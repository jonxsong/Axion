# Kubernetes deployment

The base kustomization runs one RocksDB-owning Axion pod and one web console
pod. Axion deliberately uses a single-replica `Deployment` with the `Recreate`
strategy because a RocksDB directory cannot be mounted by multiple writers.
The data directory is a `ReadWriteOnce` persistent volume claim. Horizontal
scale requires the sharding layer rather than increasing this replica count.
The server retries a transient RocksDB lock for 30 seconds during handoff from
the previous pod, which covers delayed filesystem lock release without masking
schema or configuration failures.

Before applying the base, create the referenced `axion-secrets` Secret. The
checked-in `secret.example.yaml` is a schema example only and contains no usable
credential.

```powershell
kubectl apply -f deploy/kubernetes/base/namespace.yaml
kubectl -n axion create secret generic axion-secrets --from-literal=api-key='<strong-random-value>'
kubectl apply -k deploy/kubernetes/base
kubectl -n axion rollout status deployment/axion
kubectl -n axion port-forward service/axion 8080:8080
```

Both workloads set requests and limits, restricted security contexts,
read-only root filesystems, startup/readiness/liveness probes, and disabled
service-account token mounting. Configure a production StorageClass, ingress,
TLS, external secret provider, backups, and monitoring for the target cluster.

## Validated local workflow

`make kind-test` creates or reuses a cluster named `axion`, builds and loads the
local images, creates a development Secret, applies the manifests, exercises an
authenticated API flow, deletes the live Axion pod, and verifies the record is
still searchable after the replacement pod mounts the same claim. The cluster
is intentionally retained for inspection; delete it with `kind delete cluster
--name axion` when no longer needed.

