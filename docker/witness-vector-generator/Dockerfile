FROM matterlabs/zksync-build-base:latest as builder

ARG DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/zksync
COPY . .

RUN cd prover && cargo build --release --bin zksync_witness_vector_generator

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

# copy finalization hints required for witness vector generation
COPY prover/data/keys/ /prover/data/keys/

COPY --from=builder /usr/src/zksync/prover/target/release/zksync_witness_vector_generator /usr/bin/

ENTRYPOINT ["zksync_witness_vector_generator"]
