# Will work locally only after prior universal setup key download
FROM nvidia/cuda:12.2.0-devel-ubuntu22.04 as builder

ARG DEBIAN_FRONTEND=noninteractive

ARG CUDA_ARCH=89
ENV CUDAARCHS=${CUDA_ARCH}

RUN apt-get update && apt-get install -y curl clang openssl libssl-dev gcc g++ git \
    pkg-config build-essential libclang-dev && \
    rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y && \
    rustup install nightly-2024-05-07 && \
    rustup default nightly-2024-05-07

RUN curl -Lo cmake-3.24.2-linux-x86_64.sh https://github.com/Kitware/CMake/releases/download/v3.24.2/cmake-3.24.2-linux-x86_64.sh && \
    chmod +x cmake-3.24.2-linux-x86_64.sh && \
    ./cmake-3.24.2-linux-x86_64.sh --skip-license --prefix=/usr/local

WORKDIR /usr/src/zksync
COPY . .

RUN cd prover && \
    git clone https://github.com/matter-labs/era-bellman-cuda.git --branch main bellman-cuda && \
    cmake -Bbellman-cuda/build -Sbellman-cuda/ -DCMAKE_BUILD_TYPE=Release && \
    cmake --build bellman-cuda/build/

RUN cd prover && BELLMAN_CUDA_DIR=$PWD/bellman-cuda cargo build --features "gpu" --release --bin zksync_proof_fri_compressor

FROM nvidia/cuda:12.2.0-runtime-ubuntu22.04

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

# copy VK required for proof wrapping
COPY prover/vk_setup_data_generator_server_fri/data/ /prover/vk_setup_data_generator_server_fri/data/

COPY setup_2\^24.key /setup_2\^24.key

ENV CRS_FILE=/setup_2\^24.key

COPY --from=builder /usr/src/zksync/prover/target/release/zksync_proof_fri_compressor /usr/bin/

ENTRYPOINT ["zksync_proof_fri_compressor"]
