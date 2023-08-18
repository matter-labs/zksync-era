FROM debian:bookworm-slim

ARG DEBIAN_FRONTEND=noninteractive

RUN apt update && apt install git curl clang openssl libssl-dev gcc g++ pkg-config build-essential libclang-dev -y

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# Setup rust nightly
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y && \
    rustup install nightly-2023-05-31 && \
    rustup default nightly-2023-05-31


# Setup cmake
RUN curl -Lo cmake-3.24.2-linux-x86_64.sh https://github.com/Kitware/CMake/releases/download/v3.24.2/cmake-3.24.2-linux-x86_64.sh && \
    chmod +x cmake-3.24.2-linux-x86_64.sh && \
    ./cmake-3.24.2-linux-x86_64.sh --skip-license --prefix=/usr/local
