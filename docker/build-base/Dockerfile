FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y curl clang openssl libssl-dev gcc g++ \
    pkg-config build-essential libclang-dev linux-libc-dev liburing-dev && \
    rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y && \
    rustup install nightly-2024-09-01 && \
    rustup default nightly-2024-09-01

RUN cargo install sqlx-cli --version 0.8.0
RUN cargo install sccache --version 0.8.1
