# For using private GitHub dependencies, CI downdloads all crates and bellman-cuda dependency outside of the contatiner
# Not expected to work locally

# syntax=docker/dockerfile:experimental
FROM nvidia/cuda:11.8.0-devel-ubuntu22.04 as builder

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y curl clang openssl libssl-dev gcc g++ \
    pkg-config build-essential libclang-dev && \
    rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y && \
    rustup install nightly-2023-05-31 && \
    rustup default nightly-2023-05-31

WORKDIR /usr/src/zksync
COPY . .

ENV BELLMAN_CUDA_DIR=/usr/src/zksync/bellman-cuda

RUN CARGO_HOME=./cargo cargo build --release

FROM nvidia/cuda:11.8.0-runtime-ubuntu22.04 as runner

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y libpq5 ca-certificates openssl && rm -rf /var/lib/apt/lists/*

COPY etc/system-contracts/bootloader/build/artifacts/ /etc/system-contracts/bootloader/build/artifacts/
COPY etc/system-contracts/artifacts-zk /etc/system-contracts/artifacts-zk
COPY contracts/ethereum/artifacts/ /contracts/ethereum/artifacts/
COPY contracts/zksync/artifacts-zk/ /contracts/zksync/artifacts-zk/
COPY setup_2\^26.key /etc/

COPY core/bin/verification_key_generator_and_server/data/ /core/bin/verification_key_generator_and_server/data/

COPY --from=builder /usr/src/zksync/target/release/zksync_prover /usr/bin/
ENTRYPOINT ["zksync_prover"]
