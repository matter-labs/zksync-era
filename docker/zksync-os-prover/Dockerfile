# Will work locally only after prior universal setup key download
FROM nvidia/cuda:12.2.0-devel-ubuntu22.04 AS builder

ARG DEBIAN_FRONTEND=noninteractive

# set of args for use of sccache
ARG SCCACHE_GCS_BUCKET=""
ARG SCCACHE_GCS_SERVICE_ACCOUNT=""
ARG SCCACHE_GCS_RW_MODE=""
ARG RUSTC_WRAPPER=""

ENV SCCACHE_GCS_BUCKET=${SCCACHE_GCS_BUCKET}
ENV SCCACHE_GCS_SERVICE_ACCOUNT=${SCCACHE_GCS_SERVICE_ACCOUNT}
ENV SCCACHE_GCS_RW_MODE=${SCCACHE_GCS_RW_MODE}
ENV RUSTC_WRAPPER=${RUSTC_WRAPPER}

ARG CUDA_ARCH=89
ENV CUDAARCHS=${CUDA_ARCH}

RUN apt-get update && apt-get install -y curl clang openssl libssl-dev gcc g++ git \
    pkg-config build-essential libclang-dev && \
    rm -rf /var/lib/apt/lists/*

RUN curl -Lo cmake-3.30.0-linux-x86_64.sh https://github.com/Kitware/CMake/releases/download/v3.30.0/cmake-3.30.0-linux-x86_64.sh && \
    chmod +x cmake-3.30.0-linux-x86_64.sh && \
    ./cmake-3.30.0-linux-x86_64.sh --skip-license --prefix=/usr/local

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y && \
    rustup install nightly-2024-09-01 && \
    rustup default nightly-2024-09-01

# install sccache
RUN curl -Lo sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz https://github.com/mozilla/sccache/releases/download/v0.8.1/sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz && \
    tar -xzf sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz && \
    cp sccache-v0.8.1-x86_64-unknown-linux-musl/sccache /usr/local/sbin/ && \
    rm -rf sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz && \
    rm -rf sccache-v0.8.1-x86_64-unknown-linux-musl && \
    chmod +x /usr/local/sbin/sccache

WORKDIR /usr/src/zksync
COPY . .

RUN cd zkos_prover && cargo build  --release --features gpu

FROM nvidia/cuda:12.2.0-runtime-ubuntu22.04

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/zksync/zkos_prover/target/release/zkos_prover /usr/bin/
COPY execution_environment/ /execution_environment

ENTRYPOINT ["zkos_prover"]
