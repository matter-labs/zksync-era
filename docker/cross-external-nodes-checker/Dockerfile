# syntax=docker/dockerfile:experimental
FROM debian:bookworm-slim as builder

RUN apt-get update && apt-get install -y curl clang openssl libssl-dev gcc g++ \
    pkg-config build-essential libclang-dev && \
    rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y && \
    rustup install nightly-2023-08-21 && \
    rustup default nightly-2023-08-21

WORKDIR /usr/src/zksync
COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y curl ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/zksync/target/release/cross_external_nodes_checker /usr/bin

ENTRYPOINT ["cross_external_nodes_checker"]
