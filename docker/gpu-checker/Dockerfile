ARG CIRCUIT_FILE="1286_48_1_BasicCircuits_0"

FROM nvidia/cuda:12.4.0-devel-ubuntu22.04 AS builder

ARG DEBIAN_FRONTEND=noninteractive

ARG CIRCUIT_FILE
# https://arnon.dk/matching-sm-architectures-arch-and-gencode-for-various-nvidia-cards/
ARG CUDA_ARCH="75;80;89;90"
ENV CUDAARCHS=${CUDA_ARCH}

# set of args for use of sccache
ARG SCCACHE_GCS_BUCKET=""
ARG SCCACHE_GCS_SERVICE_ACCOUNT=""
ARG SCCACHE_GCS_RW_MODE=""
ARG RUSTC_WRAPPER=""

ENV SCCACHE_GCS_BUCKET=${SCCACHE_GCS_BUCKET}
ENV SCCACHE_GCS_SERVICE_ACCOUNT=${SCCACHE_GCS_SERVICE_ACCOUNT}
ENV SCCACHE_GCS_RW_MODE=${SCCACHE_GCS_RW_MODE}
ENV RUSTC_WRAPPER=${RUSTC_WRAPPER}

RUN apt-get update && apt-get install -y curl clang openssl libssl-dev gcc g++ \
    pkg-config build-essential libclang-dev jq && \
    rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y && \
    rustup install nightly-2024-09-01 && \
    rustup default nightly-2024-09-01

RUN curl -Lo cmake-3.24.2-linux-x86_64.sh https://github.com/Kitware/CMake/releases/download/v3.24.2/cmake-3.24.2-linux-x86_64.sh && \
    chmod +x cmake-3.24.2-linux-x86_64.sh && \
    ./cmake-3.24.2-linux-x86_64.sh --skip-license --prefix=/usr/local

# install sccache
RUN curl -Lo sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz https://github.com/mozilla/sccache/releases/download/v0.8.1/sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz && \
    tar -xzf sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz && \
    cp sccache-v0.8.1-x86_64-unknown-linux-musl/sccache /usr/local/sbin/ && \
    rm -rf sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz && \
    rm -rf sccache-v0.8.1-x86_64-unknown-linux-musl && \
    chmod +x /usr/local/sbin/sccache

WORKDIR /usr/src/zksync
COPY . .

RUN cd prover && cargo build --release --package zksync_gpu_checker

# Output build in CUDA architectures for debugging purposes.
RUN cuobjdump /usr/src/zksync/prover/target/release/zksync_gpu_checker

# Generate Witness Vector
RUN mkdir -p /prover/data/keys/ && eval $(./prover/extract-setup-data-keys.sh) && \
  curl -LO --output-dir /prover/data/keys/ https://storage.googleapis.com/matterlabs-setup-data-us/${gpu_short_commit_sha}/setup_basic_1_data.bin
RUN mkdir -p /prover/data/keys/ && cp -v -r prover/data/keys/commitments.json prover/data/keys/finalization_hints_basic_1.bin prover/data/keys/verification_basic_1_key.json /prover/data/keys/
# Circut file has to go into a subdirectory because `*` is added into .dockerignore file.
RUN mkdir -p /prover/data/keys/prover_jobs_fri/ && cp -v prover/${CIRCUIT_FILE}.bin /prover/data/keys/prover_jobs_fri/

RUN /usr/src/zksync/prover/target/release/zksync_gpu_checker --object-store-path=/prover/data/keys/ --keystore-path=/prover/data/keys/ --circuit-file=/prover/data/keys/${CIRCUIT_FILE}.bin

# Final image
FROM nvidia/cuda:12.4.0-runtime-ubuntu22.04
ARG CIRCUIT_FILE

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy required setup key and circuit.
COPY --from=builder /prover/data/keys/ /prover/data/keys/
COPY --from=builder /usr/src/zksync/${CIRCUIT_FILE}.witness_vector /

COPY --from=builder /usr/src/zksync/prover/target/release/zksync_gpu_checker /usr/bin/

ENV CMD_LINE="/usr/bin/zksync_gpu_checker --object-store-path=/prover/data/keys/ --keystore-path=/prover/data/keys/ --witness-vector-file=/${CIRCUIT_FILE}.witness_vector"
CMD $CMD_LINE
