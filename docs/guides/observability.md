# Observability

Axion emits structured JSON logs to standard output, Prometheus metrics at
`GET /metrics`, and optional OpenTelemetry traces over OTLP/gRPC.

Every HTTP response carries `x-request-id`. The completion log includes that
identifier, method, path, status, and elapsed milliseconds so an operator can
correlate a client failure with service logs. Request IDs are deliberately not
used as metric labels.

## Prometheus

The exporter uses bounded labels only:

- `axion_http_requests_total{method,status}`
- `axion_http_request_duration_seconds{method}`
- `axion_searches_total{mode}`
- `axion_search_duration_seconds{mode,filter}`
- `axion_vectors_written_total{operation}`
- `axion_vector_write_duration_seconds{operation}`

Start the optional local scraper with:

```powershell
docker compose --profile observability up --build
```

Prometheus is available at `http://localhost:9090`. Histograms expose buckets,
sum, and count; derive percentiles with `histogram_quantile` in PromQL rather
than presenting client-side averages as percentiles.

## OpenTelemetry

Set `OTEL_EXPORTER_OTLP_ENDPOINT` to enable OTLP/gRPC export. With the variable
unset or empty, Axion creates no exporter and has no telemetry-network
dependency.

```powershell
$env:OTEL_EXPORTER_OTLP_ENDPOINT = "http://localhost:4317"
$env:OTEL_SERVICE_NAME = "axion-server"
docker compose up --build
```

The endpoint should be an OpenTelemetry Collector or compatible backend. Axion
shuts down its tracer provider during graceful process termination so queued
spans receive a final flush attempt. Export failure never changes search or
durability semantics.
