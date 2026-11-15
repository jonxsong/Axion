.PHONY: check web-check compose-test ann-benchmarks kind-test e2e

check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace
	cd sdks/python && .venv/Scripts/ruff check . && .venv/Scripts/pytest
	pnpm web:check

web-check:
	pnpm web:check

compose-test:
	powershell -NoProfile -ExecutionPolicy Bypass -File scripts/compose-smoke.ps1

ann-benchmarks:
	powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-ann-benchmarks.ps1

kind-test:
	powershell -NoProfile -ExecutionPolicy Bypass -File scripts/kind-smoke.ps1

e2e:
	powershell -NoProfile -ExecutionPolicy Bypass -File scripts/e2e.ps1

