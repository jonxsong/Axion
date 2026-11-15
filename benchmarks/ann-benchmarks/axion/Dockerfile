FROM ann-benchmarks AS axion-builder

ARG AXION_REPOSITORY=https://github.com/jonxsong/Axion.git
ARG AXION_REVISION=main
ARG RUST_VERSION=1.89.0
RUN apt-get update \
    && apt-get install -y --no-install-recommends clang cmake curl libclang-dev \
    && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 --fail --silent --show-error https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain "${RUST_VERSION}"
ENV PATH=/root/.cargo/bin:$PATH \
    RUSTUP_TOOLCHAIN=1.89.0
WORKDIR /src
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/src/target \
    git clone "${AXION_REPOSITORY}" axion \
    && cd axion \
    && git checkout "${AXION_REVISION}" \
    && CARGO_TARGET_DIR=/src/target cargo build --locked --release --package axion-server \
    && cp /src/target/release/axion-server /tmp/axion-server

FROM ann-benchmarks
COPY --from=axion-builder /tmp/axion-server /usr/local/bin/axion-server

