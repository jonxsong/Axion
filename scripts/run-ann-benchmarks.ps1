$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$frameworkRevision = "2e081ad32c1eccab72dcb739ad886c310b90f715"
$framework = Join-Path $root ".cache\ann-benchmarks"
$dataset = if ($env:AXION_ANN_DATASET) { $env:AXION_ANN_DATASET } else { "random-xs-20-euclidean" }
$runs = if ($env:AXION_ANN_RUNS) { $env:AXION_ANN_RUNS } else { "3" }
$timestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss")
$output = Join-Path $root "benchmarks\results\$timestamp"
$definitions = Join-Path $framework "axion-comparison-definitions"

if (-not (Test-Path -LiteralPath $framework)) {
    New-Item -ItemType Directory -Force -Path (Split-Path $framework) | Out-Null
    git clone https://github.com/erikbern/ann-benchmarks.git $framework
}
git -C $framework fetch origin $frameworkRevision --depth 1
if ($LASTEXITCODE -ne 0) { throw "Could not fetch the pinned ANN-Benchmarks revision" }
git -C $framework checkout --detach $frameworkRevision
if ($LASTEXITCODE -ne 0) { throw "Could not checkout the pinned ANN-Benchmarks revision" }

$adapter = Join-Path $framework "ann_benchmarks\algorithms\axion"
New-Item -ItemType Directory -Force -Path $adapter | Out-Null
Copy-Item -Force (Join-Path $root "benchmarks\ann-benchmarks\axion\*") $adapter
New-Item -ItemType Directory -Force -Path (Join-Path $definitions "axion") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $definitions "hnswlib") | Out-Null
Copy-Item -Force (Join-Path $root "benchmarks\ann-benchmarks\axion\config.yml") (Join-Path $definitions "axion\config.yml")
Copy-Item -Force (Join-Path $root "benchmarks\ann-benchmarks\comparison\hnswlib\config.yml") (Join-Path $definitions "hnswlib\config.yml")

Push-Location $framework
try {
    if (-not (Test-Path -LiteralPath ".venv")) { py -3.12 -m venv .venv }
    & ".venv\Scripts\python.exe" -m pip install --quiet --disable-pip-version-check -r requirements.txt
    if ($LASTEXITCODE -ne 0) { throw "ANN-Benchmarks Python dependencies failed to install" }

    docker image inspect ann-benchmarks *> $null
    if ($LASTEXITCODE -ne 0) {
        docker build --rm --tag ann-benchmarks --file ann_benchmarks/algorithms/base/Dockerfile .
        if ($LASTEXITCODE -ne 0) { throw "ANN-Benchmarks base image failed to build" }
    }
    docker build --rm --tag ann-benchmarks-axion --build-arg "AXION_REVISION=$(git -C $root rev-parse HEAD)" --file ann_benchmarks/algorithms/axion/Dockerfile .
    if ($LASTEXITCODE -ne 0) { throw "Axion ANN-Benchmarks image failed to build" }
    docker build --rm --tag ann-benchmarks-hnswlib --file ann_benchmarks/algorithms/hnswlib/Dockerfile .
    if ($LASTEXITCODE -ne 0) { throw "hnswlib ANN-Benchmarks image failed to build" }

    foreach ($algorithm in @("axion", "hnswlib")) {
        & ".venv\Scripts\python.exe" run.py --dataset $dataset --definitions $definitions --algorithm $algorithm --runs $runs --force
        if ($LASTEXITCODE -ne 0) { throw "$algorithm ANN-Benchmarks run failed" }
    }

    New-Item -ItemType Directory -Force -Path $output | Out-Null
    & ".venv\Scripts\python.exe" data_export.py --output (Join-Path $output "results.csv") --recompute
    if ($LASTEXITCODE -ne 0) { throw "ANN-Benchmarks CSV export failed" }
    & ".venv\Scripts\python.exe" plot.py --dataset $dataset --definitions $definitions --x-axis k-nn --y-axis qps --y-scale log --raw --recompute --output (Join-Path $output "recall-vs-qps.png")
    if ($LASTEXITCODE -ne 0) { throw "ANN-Benchmarks plot failed" }

    $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1 Name, NumberOfCores, NumberOfLogicalProcessors
    $system = Get-CimInstance Win32_ComputerSystem
    $metadata = [ordered]@{
        dataset = $dataset
        runs = [int]$runs
        count = 10
        framework_repository = "https://github.com/erikbern/ann-benchmarks"
        framework_revision = $frameworkRevision
        axion_revision = (git -C $root rev-parse HEAD)
        timestamp_utc = (Get-Date).ToUniversalTime().ToString("o")
        cpu = $cpu.Name
        physical_cores = $cpu.NumberOfCores
        logical_processors = $cpu.NumberOfLogicalProcessors
        memory_bytes = [uint64]$system.TotalPhysicalMemory
        runner = "official Docker runner, one pinned CPU per algorithm"
    }
    $metadata | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $output "run-metadata.json")
} finally {
    Pop-Location
}

Write-Host "ANN-Benchmarks comparison passed. Results: $output"
