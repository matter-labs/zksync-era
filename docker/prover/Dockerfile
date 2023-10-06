FROM nvidia/cuda:11.8.0-devel-ubuntu22.04 as builder

ARG DEBIAN_FRONTEND=noninteractive

ARG ERA_BELLMAN_CUDA_RELEASE=latest

RUN apt-get update && apt-get install -y curl clang openssl libssl-dev gcc g++ \
    pkg-config build-essential libclang-dev && \
    rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y && \
    rustup install nightly-2023-07-21 && \
    rustup default nightly-2023-07-21

WORKDIR /usr/src/zksync
COPY . .

RUN set -e; \
    if [ "$ERA_BELLMAN_CUDA_RELEASE" = "latest" ]; then \
    api_url="https://api.github.com/repos/matter-labs/era-bellman-cuda/releases/latest"; \
    release_info=$(curl -s -o /dev/null -w "%{http_code}" "$api_url"); \
    if [ "$release_info" = "404" ]; then \
    api_url="https://api.github.com/repos/matter-labs/era-bellman-cuda/releases"; \
    release_info=$(curl --silent "$api_url" | jq -r '.[0].tarball_url, .[0].assets[0].browser_download_url'); \
    else \
    release_info=$(curl --silent "$api_url" | jq -r ".tarball_url, .assets[0].browser_download_url"); \
    fi; \
    else \
    api_url="https://api.github.com/repos/matter-labs/era-bellman-cuda/releases/tags/${ERA_BELLMAN_CUDA_RELEASE}"; \
    release_info=$(curl --silent "$api_url" | jq -r ".tarball_url, .assets[0].browser_download_url"); \
    fi; \
    IFS=$'\n' read -ra release <<< "$release_info"; \
    source="${release[0]}"; \
    binary="${release[1]}"; \
    curl --silent --location "$source" --output bellman-cuda-source.tar.gz; \
    curl --silent --location "$binary" --output bellman-cuda.tar.gz; \
    mkdir -p bellman-cuda; \
    tar xvfz bellman-cuda.tar.gz -C ./bellman-cuda; \
    tar xvfz bellman-cuda-source.tar.gz -C ./bellman-cuda --strip-components=1

RUN cargo build --release

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
