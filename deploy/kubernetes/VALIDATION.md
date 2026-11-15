# kind validation — 2026-11-15

The mandatory local Kubernetes workflow passed on:

- kind v0.33.0
- Kubernetes client v1.36.1 and kind node/server v1.37.0
- Docker Desktop engine 29.9.3
- Windows 11 host

`scripts/kind-smoke.ps1` built the local production images, loaded them into a
cluster named `axion`, created the referenced Secret, and applied the base
kustomization under the `restricted` Pod Security profile.

Observed acceptance evidence:

- `deployment/axion`: 1/1 Ready
- `deployment/axion-web`: 1/1 Ready
- `pvc/axion-data`: Bound, 2 GiB, `ReadWriteOnce`, `standard` StorageClass
- REST and gRPC ClusterIP service ports: 8080 and 50051
- Web ClusterIP service port: 3000
- Authenticated collection and vector writes: passed
- Pod replacement: `axion-7b4bc7b869-5sh5b` to
  `axion-7b4bc7b869-67nn6`
- RocksDB recovery: the pre-replacement `persistent` record was returned by an
  approximate search after replacement
- Replacement pod stability: Ready with zero container restarts after the
  post-recovery observation interval
- Web landing page through the Service: HTTP 200 with expected product content

An initial replacement run exposed a short RocksDB lock-release race on the
kind hostPath volume. Axion now retries only transient RocksDB lock errors for a
bounded 30-second window. A real double-open unit test covers this behavior;
schema and configuration errors still fail immediately.

The kind cluster and PVC were intentionally retained after validation for
inspection. They are local validation infrastructure, not part of the source
artifact.
