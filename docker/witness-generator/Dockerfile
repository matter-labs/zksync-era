# For using private GitHub dependencies, CI downdloads all crates outside of the contatiner
# Not expected to work locally

# syntax=docker/dockerfile:experimental
FROM rust:1.67-buster as builder
RUN apt-get update && apt-get install -y clang && rm -rf /var/lib/apt/lists/*
WORKDIR /usr/src/zksync
COPY . .

RUN CARGO_HOME=./cargo cargo build --release

FROM debian:buster-slim
RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY core/bin/verification_key_generator_and_server/data/ /core/bin/verification_key_generator_and_server/data/

COPY --from=builder /usr/src/zksync/target/release/zksync_witness_generator /usr/bin/
ENTRYPOINT ["zksync_witness_generator"]
