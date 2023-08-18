# For using private GitHub dependencies, CI downdloads all crates outside of the contatiner
# Not expected to work locally

# BUILDING STAGE
# syntax=docker/dockerfile:experimental
FROM rust:1.67-bookworm as builder
RUN apt-get update && apt-get install -y clang && rm -rf /var/lib/apt/lists/*
WORKDIR /usr/src/zksync
COPY . .

RUN CARGO_HOME=./cargo cargo build --release

# RUNNING STAGE
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y curl ca-certificates && rm -rf /var/lib/apt/lists/*

# Bring the below from the building stage to the final image.
COPY --from=builder /usr/src/zksync/target/release/cross_external_nodes_checker /usr/bin

# Run the entrypoint script as the default command.
ENTRYPOINT ["cross_external_nodes_checker"]
