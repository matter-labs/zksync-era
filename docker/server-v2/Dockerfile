# Will work locally only after prior contracts build
# syntax=docker/dockerfile:experimental
FROM debian:bookworm-slim as builder

WORKDIR /usr/src/zksync
COPY . .

RUN apt-get update && apt-get install -y curl clang openssl libssl-dev gcc g++ \
    pkg-config build-essential libclang-dev linux-libc-dev liburing-dev && \
    rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y && \
    rustup install nightly-2023-08-21 && \
    rustup default nightly-2023-08-21

RUN cargo build --release --features=rocksdb/io-uring

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y curl libpq5 liburing-dev ca-certificates && \
    rm -rf /var/lib/apt/lists/*

EXPOSE 3000
EXPOSE 3031
EXPOSE 3030

COPY --from=builder /usr/src/zksync/target/release/zksync_server /usr/bin
COPY --from=builder /usr/src/zksync/target/release/block_reverter /usr/bin
COPY --from=builder /usr/src/zksync/target/release/merkle_tree_consistency_checker /usr/bin
COPY --from=builder /usr/src/zksync/target/release/rocksdb_util /usr/bin
COPY etc/system-contracts/bootloader/build/artifacts/ /etc/system-contracts/bootloader/build/artifacts/
COPY etc/system-contracts/contracts/artifacts/ /etc/system-contracts/contracts/artifacts/
COPY etc/system-contracts/contracts/precompiles/artifacts/ /etc/system-contracts/contracts/precompiles/artifacts/
COPY etc/system-contracts/artifacts-zk /etc/system-contracts/artifacts-zk
COPY contracts/ethereum/artifacts/ /contracts/ethereum/artifacts/
COPY contracts/zksync/artifacts-zk/ /contracts/zksync/artifacts-zk/
COPY etc/tokens/ /etc/tokens/
COPY etc/ERC20/ /etc/ERC20/
COPY etc/multivm_bootloaders/ /etc/multivm_bootloaders/
COPY core/bin/verification_key_generator_and_server/data/ /core/bin/verification_key_generator_and_server/data/

ENTRYPOINT ["zksync_server"]
