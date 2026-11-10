FROM ghcr.io/matter-labs/zksync-build-base:latest AS builder

ARG DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/zksync
COPY . .

RUN cd prover && CARGO_HOME=/usr/src/zksync/cargo cargo build --locked --release --bin zksync_prover_fri_gateway

FROM ghcr.io/matter-labs/zksync-runtime-base:latest

# copy VK required for proof wrapping
COPY prover/data/keys/ /prover/data/keys/

COPY --from=builder /usr/src/zksync/prover/target/release/zksync_prover_fri_gateway /usr/bin/

ENTRYPOINT ["zksync_prover_fri_gateway"]
