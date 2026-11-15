$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$cluster = "axion"
$context = "kind-$cluster"
$namespace = "axion"
$apiKey = "kind-smoke-key"
$apiPort = 18084
$webPort = 3003
$collection = "kind_$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())"
$apiForward = $null
$webForward = $null

function Refresh-Path {
    $machine = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $user = [Environment]::GetEnvironmentVariable("Path", "User")
    $env:Path = "$machine;$user"
}

function Wait-HttpReady {
    param([string]$Uri, [int]$Attempts = 60)
    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        try {
            $response = Invoke-RestMethod -Uri $Uri -TimeoutSec 2
            if ($response.status -in @("ready", "ok")) { return }
        } catch { }
        Start-Sleep -Seconds 2
    }
    throw "Endpoint did not become ready: $Uri"
}

function Start-PortForward {
    param([string]$Resource, [string]$Ports)
    return Start-Process -FilePath "kubectl" -ArgumentList @(
        "--context", $context, "--namespace", $namespace,
        "port-forward", $Resource, $Ports, "--address", "127.0.0.1"
    ) -PassThru -WindowStyle Hidden
}

Refresh-Path
foreach ($command in @("docker", "kind", "kubectl")) {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) { throw "$command is required" }
}

docker build --tag axion-server:local --file (Join-Path $root "Dockerfile") $root
if ($LASTEXITCODE -ne 0) { throw "Axion image build failed" }
docker build --tag axion-web:local --file (Join-Path $root "apps\web\Dockerfile") $root
if ($LASTEXITCODE -ne 0) { throw "Web image build failed" }

if ((kind get clusters) -notcontains $cluster) {
    kind create cluster --name $cluster --wait 180s
    if ($LASTEXITCODE -ne 0) { throw "kind cluster creation failed" }
}
kind load docker-image axion-server:local axion-web:local --name $cluster
if ($LASTEXITCODE -ne 0) { throw "kind image load failed" }

kubectl --context $context apply -f (Join-Path $root "deploy\kubernetes\base\namespace.yaml")
kubectl --context $context --namespace $namespace create secret generic axion-secrets --from-literal="api-key=$apiKey" --dry-run=client --output=yaml | kubectl --context $context apply -f -
if ($LASTEXITCODE -ne 0) { throw "Secret creation failed" }
kubectl --context $context apply -k (Join-Path $root "deploy\kubernetes\base")
if ($LASTEXITCODE -ne 0) { throw "Kubernetes apply failed" }
kubectl --context $context --namespace $namespace rollout status deployment/axion --timeout=180s
if ($LASTEXITCODE -ne 0) { throw "Axion rollout failed" }
kubectl --context $context --namespace $namespace rollout status deployment/axion-web --timeout=180s
if ($LASTEXITCODE -ne 0) { throw "Web rollout failed" }

try {
    $apiForward = Start-PortForward "service/axion" "$apiPort`:8080"
    Wait-HttpReady "http://127.0.0.1:$apiPort/health/ready"

    $headers = @{ Authorization = "Bearer $apiKey" }
    $collectionBody = @{ name = $collection; dimensions = 3; metric = "cosine" } | ConvertTo-Json
    Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:$apiPort/v1/collections" -Headers $headers -ContentType "application/json" -Body $collectionBody | Out-Null
    $vectorBody = @{ id = "persistent"; values = @(1.0, 0.0, 0.0); metadata = @{ validation = "kind-pod-replacement" } } | ConvertTo-Json -Depth 5
    Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:$apiPort/v1/collections/$collection/vectors" -Headers $headers -ContentType "application/json" -Body $vectorBody | Out-Null

    $oldPod = kubectl --context $context --namespace $namespace get pods -l "app.kubernetes.io/name=axion,app.kubernetes.io/component=engine" -o jsonpath="{.items[0].metadata.name}"
    kubectl --context $context --namespace $namespace delete pod $oldPod --wait=true
    if ($LASTEXITCODE -ne 0) { throw "Pod replacement request failed" }
    if ($apiForward -and -not $apiForward.HasExited) { Stop-Process -Id $apiForward.Id -Force }
    $apiForward = $null
    kubectl --context $context --namespace $namespace rollout status deployment/axion --timeout=180s
    if ($LASTEXITCODE -ne 0) { throw "Replacement Axion pod did not become ready" }

    $newPod = kubectl --context $context --namespace $namespace get pods -l "app.kubernetes.io/name=axion,app.kubernetes.io/component=engine" -o jsonpath="{.items[0].metadata.name}"
    if ($newPod -eq $oldPod) { throw "Axion pod was not replaced" }
    $apiForward = Start-PortForward "service/axion" "$apiPort`:8080"
    Wait-HttpReady "http://127.0.0.1:$apiPort/health/ready"
    $searchBody = @{ vector = @(1.0, 0.0, 0.0); top_k = 1; mode = "approximate" } | ConvertTo-Json
    $search = Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:$apiPort/v1/collections/$collection/search" -Headers $headers -ContentType "application/json" -Body $searchBody
    if ($search.results[0].id -ne "persistent") { throw "Persisted record was not recovered after pod replacement" }
    Start-Sleep -Seconds 10
    $restartCount = kubectl --context $context --namespace $namespace get pod $newPod -o jsonpath="{.status.containerStatuses[0].restartCount}"
    if ([int]$restartCount -ne 0) { throw "Replacement Axion pod restarted $restartCount time(s)" }

    $webForward = Start-PortForward "service/axion-web" "$webPort`:3000"
    for ($attempt = 1; $attempt -le 30; $attempt++) {
        try {
            $web = Invoke-WebRequest -Uri "http://127.0.0.1:$webPort" -TimeoutSec 2 -UseBasicParsing
            if ($web.StatusCode -eq 200 -and $web.Content -match "Vector search") { break }
        } catch { }
        if ($attempt -eq 30) { throw "Web console did not become ready" }
        Start-Sleep -Seconds 2
    }

    Write-Host "kind smoke passed: PVC retained $collection across $oldPod -> $newPod with zero container restarts."
} finally {
    foreach ($process in @($apiForward, $webForward)) {
        if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    }
}
