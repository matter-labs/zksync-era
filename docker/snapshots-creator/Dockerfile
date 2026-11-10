# syntax=docker/dockerfile:experimental
FROM ghcr.io/matter-labs/zksync-build-base:latest AS builder

WORKDIR /usr/src/zksync
COPY . .

RUN CARGO_HOME=/usr/src/zksync/cargo cargo build --locked --manifest-path ./core/Cargo.toml --release --bin snapshots_creator

FROM ghcr.io/matter-labs/zksync-runtime-base:latest

RUN apt-get update && apt-get install -y liburing-dev && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/zksync/core/target/release/snapshots_creator /usr/bin

ENTRYPOINT ["snapshots_creator"]
