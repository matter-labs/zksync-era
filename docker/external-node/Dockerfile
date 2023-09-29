# For using private GitHub dependencies, CI downdloads all crates outside of the contatiner
# Not expected to work locally

# syntax=docker/dockerfile:experimental
FROM debian:bookworm-slim as builder

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

RUN CARGO_HOME=./cargo cargo build --release
RUN CARGO_HOME=./cargo cargo install sqlx-cli --version 0.5.13

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/zksync/target/release/zksync_external_node /usr/bin
COPY --from=builder /usr/src/zksync/target/release/block_reverter /usr/bin
COPY --from=builder /usr/src/zksync/cargo/bin/sqlx /usr/bin
COPY --from=builder /usr/src/zksync/docker/external-node/entrypoint.sh /usr/bin
RUN chmod +x /usr/bin/entrypoint.sh
COPY etc/system-contracts/bootloader/build/artifacts/ /etc/system-contracts/bootloader/build/artifacts/
COPY etc/system-contracts/contracts/artifacts/ /etc/system-contracts/contracts/artifacts/
COPY etc/system-contracts/contracts/precompiles/artifacts/ /etc/system-contracts/contracts/precompiles/artifacts/
COPY etc/system-contracts/artifacts-zk /etc/system-contracts/artifacts-zk
COPY contracts/ethereum/artifacts/ /contracts/ethereum/artifacts/
COPY contracts/zksync/artifacts-zk/ /contracts/zksync/artifacts-zk/
COPY etc/tokens/ /etc/tokens/
COPY etc/ERC20/ /etc/ERC20/
COPY etc/multivm_bootloaders/ /etc/multivm_bootloaders/
COPY core/lib/dal/migrations/ /migrations

ENTRYPOINT [ "sh", "/usr/bin/entrypoint.sh"]
