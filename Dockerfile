FROM rust:1.100-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends clang cmake libclang-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY proto ./proto
COPY docs/api ./docs/api
RUN cargo build --locked --release --package axion-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 axion \
    && useradd --uid 10001 --gid axion --shell /usr/sbin/nologin --create-home axion \
    && mkdir -p /var/lib/axion \
    && chown axion:axion /var/lib/axion

COPY --from=builder /src/target/release/axion-server /usr/local/bin/axion-server

USER 10001:10001
ENV AXION_HOST=0.0.0.0 \
    AXION_HTTP_PORT=8080 \
    AXION_GRPC_PORT=50051 \
    AXION_DATA_DIR=/var/lib/axion \
    RUST_LOG=axion=info
VOLUME ["/var/lib/axion"]
EXPOSE 8080 50051
HEALTHCHECK --interval=10s --timeout=3s --start-period=20s --retries=6 \
    CMD curl --fail --silent http://127.0.0.1:8080/health/ready >/dev/null || exit 1
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/axion-server"]

