$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$project = "axion-e2e"
$env:AXION_HTTP_PORT = "18085"
$env:AXION_GRPC_PORT = "50065"
$env:AXION_WEB_PORT = "3004"
$env:AXION_DATA_VOLUME = "axion-e2e-data"
$env:AXION_PROMETHEUS_VOLUME = "axion-e2e-prometheus-data"
$env:AXION_E2E_BASE_URL = "http://127.0.0.1:3004"
$env:AXION_E2E_PROJECT = $project

Push-Location $root
try {
    docker compose -p $project up --build --detach
    if ($LASTEXITCODE -ne 0) { throw "E2E Compose startup failed" }
    pnpm --filter @axion/web e2e
    if ($LASTEXITCODE -ne 0) { throw "Playwright acceptance failed" }
} finally {
    docker compose -p $project down --volumes --remove-orphans
    Pop-Location
}
