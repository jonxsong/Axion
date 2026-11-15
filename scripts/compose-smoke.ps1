$ErrorActionPreference = "Stop"

$httpPort = if ($env:AXION_HTTP_PORT) { $env:AXION_HTTP_PORT } else { "8080" }
$webPort = if ($env:AXION_WEB_PORT) { $env:AXION_WEB_PORT } else { "3000" }
$baseUrl = "http://127.0.0.1:$httpPort"
$webUrl = "http://127.0.0.1:$webPort"
$collection = "persistence_$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())"

function Wait-AxionHealthy {
    param([int]$Attempts = 60)
    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        try {
            $health = Invoke-RestMethod -Uri "$baseUrl/health/ready" -TimeoutSec 2
            if ($health.status -eq "ready") { return }
        } catch { }
        Start-Sleep -Seconds 2
    }
    throw "Axion did not become ready"
}

docker compose up --detach --build
if ($LASTEXITCODE -ne 0) { throw "docker compose up failed" }
Wait-AxionHealthy

$collectionBody = @{ name = $collection; dimensions = 3; metric = "cosine" } | ConvertTo-Json
Invoke-RestMethod -Method Post -Uri "$baseUrl/v1/collections" -ContentType "application/json" -Body $collectionBody | Out-Null
$vectorBody = @{ id = "durable-vector"; values = @(1.0, 0.0, 0.0); metadata = @{ test = "compose-restart" } } | ConvertTo-Json -Depth 5
Invoke-RestMethod -Method Post -Uri "$baseUrl/v1/collections/$collection/vectors" -ContentType "application/json" -Body $vectorBody | Out-Null

docker compose restart axion
if ($LASTEXITCODE -ne 0) { throw "docker compose restart failed" }
Wait-AxionHealthy

$searchBody = @{ vector = @(1.0, 0.0, 0.0); top_k = 1; mode = "approximate" } | ConvertTo-Json
$search = Invoke-RestMethod -Method Post -Uri "$baseUrl/v1/collections/$collection/search" -ContentType "application/json" -Body $searchBody
if ($search.results[0].id -ne "durable-vector") { throw "persistence search returned the wrong vector" }

$web = Invoke-WebRequest -Uri $webUrl -TimeoutSec 5 -UseBasicParsing
if ($web.StatusCode -ne 200 -or $web.Content -notmatch "Vector search") { throw "web application smoke test failed" }

Write-Host "Compose smoke passed: $collection survived an Axion container restart."

