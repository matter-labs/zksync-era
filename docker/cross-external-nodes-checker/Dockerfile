FROM rust:1.72-bookworm as builder

RUN apt-get update && apt-get install -y clang && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/zksync
COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y curl ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/zksync/target/release/cross_external_nodes_checker /usr/bin

ENTRYPOINT ["cross_external_nodes_checker"]
