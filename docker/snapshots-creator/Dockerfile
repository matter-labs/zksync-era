# syntax=docker/dockerfile:experimental
FROM matterlabs/zksync-build-base:latest as builder

WORKDIR /usr/src/zksync
COPY . .

RUN cargo build --release --bin snapshots_creator

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y curl libpq5 liburing-dev ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/zksync/target/release/snapshots_creator /usr/bin

ENTRYPOINT ["snapshots_creator"]
