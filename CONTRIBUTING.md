# Contributing to Axion

Thank you for improving Axion. Open an issue before a large behavioral or storage-format change so the contract and migration path can be agreed first. Security reports belong in the private process described by [SECURITY.md](SECURITY.md), not a public issue.

## Development setup

Install Rust stable with rustfmt and Clippy, Clang/libclang, CMake, Node.js 24, pnpm 11.25, Python 3.12+, Docker Compose, kubectl, and kind. Then:

```powershell
python -m venv sdks/python/.venv
sdks/python/.venv/Scripts/python -m pip install -e "./sdks/python[dev]"
pnpm install --frozen-lockfile
make check
```

On Linux or macOS, use the virtual environment's `bin/python`; the Makefile's Python target currently follows the Windows workspace convention, while CI executes the equivalent commands directly.

## Pull-request expectations

- Preserve RocksDB as the default durable backend and document any storage-format change with an ADR and migration.
- Add deterministic unit tests for core/index behavior and real integration coverage at API or deployment boundaries.
- Run `make check`; for relevant changes also run `make compose-test`, `make distributed-test`, `make kind-test`, or `make e2e`.
- Use Criterion for Rust microbenchmarks and official ANN-Benchmarks for cross-implementation search comparisons. Include raw evidence and host/configuration metadata with performance claims.
- Keep public errors useful but free of internal paths, credentials, and raw transport details.
- Update OpenAPI, protobuf, SDK types, docs, and examples together when changing a public contract.

Commits should be focused and descriptive. Do not rewrite shared history. By contributing, you agree that your work is licensed under the repository's MIT License.
