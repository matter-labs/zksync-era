FROM matterlabs/zksync-build-base:latest AS builder

ARG DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/zksync
COPY . .

RUN cd prover && cargo build --release --bin zksync_witness_generator

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY prover/vk_setup_data_generator_server_fri/data/ /prover/vk_setup_data_generator_server_fri/data/

COPY --from=builder /usr/src/zksync/prover/target/release/zksync_witness_generator /usr/bin/

ENTRYPOINT ["zksync_witness_generator"]
