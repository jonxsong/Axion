# Quickstart

## Requirements

The fastest supported path requires Docker Desktop with Compose v2. Native development additionally needs Rust stable, CMake, Clang/libclang, Node.js 24 with pnpm 11, and Python 3.12+.

## Start Axion

```powershell
git clone https://github.com/jonxsong/Axion.git
cd Axion
docker compose up --build -d
```

The web console is at `http://localhost:3000`, REST at `http://localhost:8080`, gRPC at `localhost:50051`, OpenAPI at `http://localhost:8080/openapi.yaml`, and Swagger UI at `http://localhost:8080/docs`.

## Create, write, and search

```powershell
$collection = @{ name = "documents"; dimensions = 3; metric = "cosine" } | ConvertTo-Json
Invoke-RestMethod -Method Post -Uri http://localhost:8080/v1/collections -ContentType application/json -Body $collection

$record = @{ id = "doc-1"; values = @(1.0, 0.0, 0.0); metadata = @{ team = "search" } } | ConvertTo-Json -Depth 4
Invoke-RestMethod -Method Post -Uri http://localhost:8080/v1/collections/documents/vectors -ContentType application/json -Body $record

$query = @{ vector = @(1.0, 0.0, 0.0); top_k = 5; mode = "approximate" } | ConvertTo-Json
Invoke-RestMethod -Method Post -Uri http://localhost:8080/v1/collections/documents/search -ContentType application/json -Body $query
```

## Verify persistence

```powershell
docker compose restart axion
./scripts/compose-smoke.ps1
```

The smoke writes a unique record, restarts the server container, verifies the record through search, and checks the web application. Compose stores RocksDB in the `axion-data` named volume; `docker compose down` preserves it, while `docker compose down --volumes` intentionally deletes it.
