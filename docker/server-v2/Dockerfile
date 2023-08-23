# For using private GitHub dependencies, CI downdloads all crates outside of the contatiner
# Not expected to work locally

# syntax=docker/dockerfile:experimental
FROM rust:1.67-bookworm as builder
RUN apt-get update && apt-get install -y linux-libc-dev liburing-dev clang && \
    # ^ We need a newer version of `linux-libc-dev` from backports than the one installed by default
    rm -rf /var/lib/apt/lists/*
WORKDIR /usr/src/zksync
COPY . .

RUN CARGO_HOME=./cargo cargo build --release --features=rocksdb/io-uring

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y curl libpq5 liburing-dev ca-certificates && \
    rm -rf /var/lib/apt/lists/*
EXPOSE 3000
EXPOSE 3031
EXPOSE 3030
COPY --from=builder /usr/src/zksync/target/release/zksync_server /usr/bin
COPY --from=builder /usr/src/zksync/target/release/block_reverter /usr/bin
COPY --from=builder /usr/src/zksync/target/release/merkle_tree_consistency_checker /usr/bin
COPY --from=builder /usr/src/zksync/target/release/slot_index_consistency_checker /usr/bin
COPY --from=builder /usr/src/zksync/target/release/rocksdb_util /usr/bin
COPY etc/system-contracts/bootloader/build/artifacts/ /etc/system-contracts/bootloader/build/artifacts/
COPY etc/system-contracts/contracts/artifacts/ /etc/system-contracts/contracts/artifacts/
COPY etc/system-contracts/contracts/precompiles/artifacts/ /etc/system-contracts/contracts/precompiles/artifacts/
COPY etc/system-contracts/artifacts-zk /etc/system-contracts/artifacts-zk
COPY contracts/ethereum/artifacts/ /contracts/ethereum/artifacts/
COPY contracts/zksync/artifacts-zk/ /contracts/zksync/artifacts-zk/
COPY etc/tokens/ /etc/tokens/
COPY etc/ERC20/ /etc/ERC20/
COPY core/bin/verification_key_generator_and_server/data/ /core/bin/verification_key_generator_and_server/data/

ENTRYPOINT ["zksync_server"]
