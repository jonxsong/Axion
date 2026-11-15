# Docker and Compose

The root `Dockerfile` is a multi-stage build. Its runtime image contains the single-node `axion-server` and optional `axion-cluster` coordinator, runs as UID/GID 10001, and includes only runtime certificates and curl. The default command starts the server.

```powershell
docker compose up --build -d
docker compose ps
docker compose logs -f axion
```

The default topology runs Axion, the Next.js console, and a durable named RocksDB volume. Enable the optional Prometheus service with `docker compose --profile observability up -d`. Set host ports and the API key in `.env`; use `.env.example` as the reference.

Run `make compose-test` (or `./scripts/compose-smoke.ps1`) to validate a real write and a container restart. Run `make distributed-test` for the isolated coordinator/two-shard topology. Test workflows use unique Compose project names and remove only their own resources.

Before upgrading, back up the RocksDB volume at the storage layer and read release notes for schema changes. Never mount the same RocksDB directory into two live server processes.
