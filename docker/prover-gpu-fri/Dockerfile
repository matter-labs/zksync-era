# For using private GitHub dependencies, CI downdloads all crates outside of the contatiner
# Not expected to work locally

# syntax=docker/dockerfile:experimental
FROM nvidia/cuda:12.0.0-devel-ubuntu22.04 as builder

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y curl clang openssl libssl-dev gcc g++ \
    pkg-config build-essential libclang-dev && \
    rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

ENV CUDAARCHS=75

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y && \
    rustup install nightly-2023-05-31 && \
    rustup default nightly-2023-05-31

# Setup cmake
RUN curl -Lo cmake-3.24.2-linux-x86_64.sh https://github.com/Kitware/CMake/releases/download/v3.24.2/cmake-3.24.2-linux-x86_64.sh && \
    chmod +x cmake-3.24.2-linux-x86_64.sh && \
    ./cmake-3.24.2-linux-x86_64.sh --skip-license --prefix=/usr/local

WORKDIR /usr/src/zksync
COPY . .

RUN CARGO_HOME=./cargo cargo build --release --features "gpu"

FROM nvidia/cuda:12.0.0-devel-ubuntu22.04
RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/zksync/target/release/zksync_prover_fri /usr/bin/
ENTRYPOINT ["zksync_prover_fri"]
