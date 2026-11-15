$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$composeFile = Join-Path $root "deploy/docker-compose.distributed.yml"
$project = "axion-distributed-test"
$coordinator = "http://127.0.0.1:18090"
$shardA = "http://127.0.0.1:18091"
$shardB = "http://127.0.0.1:18092"
$coordinatorHeaders = @{ Authorization = "Bearer local-coordinator-key" }
$shardHeaders = @{ Authorization = "Bearer local-shard-key" }
$collection = "distributed_$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())"

function Wait-Ready {
    param([string]$Uri, [int]$Attempts = 60)
    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        try {
            $response = Invoke-RestMethod -Uri $Uri -TimeoutSec 2
            if ($response.status -eq "ready") { return }
        } catch { }
        Start-Sleep -Seconds 2
    }
    throw "$Uri did not become ready"
}

function Invoke-Json {
    param(
        [ValidateSet("Get", "Post", "Delete")][string]$Method,
        [string]$Uri,
        [hashtable]$Headers,
        [object]$Body
    )
    $arguments = @{ Method = $Method; Uri = $Uri; Headers = $Headers; TimeoutSec = 10 }
    if ($null -ne $Body) {
        $arguments.ContentType = "application/json"
        $arguments.Body = $Body | ConvertTo-Json -Depth 8 -Compress
    }
    Invoke-RestMethod @arguments
}

Push-Location $root
try {
    docker compose -p $project -f $composeFile up --detach --build
    if ($LASTEXITCODE -ne 0) { throw "distributed Compose startup failed" }
    Wait-Ready "$coordinator/health/ready"

    $config = @{ name = $collection; dimensions = 3; metric = "cosine" }
    Invoke-Json Post "$coordinator/v1/cluster/collections" $coordinatorHeaders $config | Out-Null

    $vectors = 0..19 | ForEach-Object {
        $offset = ($_ + 1) / 100.0
        @{ id = "record-$_"; values = @(1.0, $offset, 0.0); metadata = @{ ordinal = $_ } }
    }
    Invoke-Json Post "$coordinator/v1/cluster/collections/$collection/vectors/batch" $coordinatorHeaders @{ vectors = $vectors } | Out-Null

    $summaryA = Invoke-Json Get "$shardA/v1/collections/$collection" $shardHeaders $null
    $summaryB = Invoke-Json Get "$shardB/v1/collections/$collection" $shardHeaders $null
    if ($summaryA.vector_count -lt 1 -or $summaryB.vector_count -lt 1) { throw "stable hashing did not use both shards" }
    if (($summaryA.vector_count + $summaryB.vector_count) -ne 20) { throw "partitioned write count was not exactly 20" }

    $query = @{ vector = @(1.0, 0.0, 0.0); top_k = 5; mode = "exact" }
    $full = Invoke-Json Post "$coordinator/v1/cluster/collections/$collection/search" $coordinatorHeaders $query
    if ($full.partial -or $full.successful_shards -ne 2 -or $full.results.Count -ne 5) { throw "full cluster search was incomplete" }

    docker compose -p $project -f $composeFile stop shard-b
    if ($LASTEXITCODE -ne 0) { throw "failed to stop shard-b" }
    Start-Sleep -Seconds 2
    $health = Invoke-RestMethod -Uri "$coordinator/v1/cluster/health" -TimeoutSec 10
    if ($health.status -ne "degraded" -or $health.ready_shards -ne 1) { throw "coordinator did not report degraded health" }
    $partial = Invoke-Json Post "$coordinator/v1/cluster/collections/$collection/search" $coordinatorHeaders $query
    if (-not $partial.partial -or $partial.successful_shards -ne 1 -or $partial.failures[0].shard -ne "shard-b") {
        throw "degraded search did not expose partial-result metadata"
    }

    docker compose -p $project -f $composeFile start shard-b
    if ($LASTEXITCODE -ne 0) { throw "failed to restart shard-b" }
    Wait-Ready "$shardB/health/ready"
    docker compose -p $project -f $composeFile restart shard-a
    if ($LASTEXITCODE -ne 0) { throw "failed to restart shard-a" }
    Wait-Ready "$shardA/health/ready"
    Wait-Ready "$coordinator/health/ready"

    $recoveredA = Invoke-Json Get "$shardA/v1/collections/$collection" $shardHeaders $null
    $recoveredB = Invoke-Json Get "$shardB/v1/collections/$collection" $shardHeaders $null
    if (($recoveredA.vector_count + $recoveredB.vector_count) -ne 20) { throw "vectors did not survive shard restarts" }
    $recovered = Invoke-Json Post "$coordinator/v1/cluster/collections/$collection/search" $coordinatorHeaders $query
    if ($recovered.partial -or $recovered.successful_shards -ne 2) { throw "cluster did not recover after shard restarts" }

    Write-Host "Distributed smoke passed: 20 vectors partitioned across two durable shards; degraded search and recovery validated."
} finally {
    docker compose -p $project -f $composeFile down --volumes --remove-orphans
    Pop-Location
}
