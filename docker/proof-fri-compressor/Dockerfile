# Will work locally only after prior universal setup key download

FROM matterlabs/zksync-build-base:latest as builder

ARG DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/zksync
COPY . .

RUN cd prover && cargo build --release --bin zksync_proof_fri_compressor

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

# copy VK required for proof wrapping
COPY prover/vk_setup_data_generator_server_fri/data/ /prover/vk_setup_data_generator_server_fri/data/

# copy universal setup key required for proof compression
COPY setup_2\^26.key /setup_2\^26.key

COPY --from=builder /usr/src/zksync/prover/target/release/zksync_proof_fri_compressor /usr/bin/

ENTRYPOINT ["zksync_proof_fri_compressor"]
