# Will work locally only after prior contracts build

FROM ghcr.io/matter-labs/zksync-build-base:latest AS builder

WORKDIR /usr/src/zksync
COPY . .
RUN CARGO_HOME=/usr/src/zksync/cargo cargo build --locked --manifest-path ./core/Cargo.toml --release --bin zksync_external_node --bin block_reverter

FROM ghcr.io/matter-labs/zksync-runtime-base:latest

COPY --from=builder /usr/src/zksync/core/target/release/zksync_external_node /usr/bin
COPY --from=builder /usr/src/zksync/core/target/release/block_reverter /usr/bin
COPY --from=builder /usr/local/cargo/bin/sqlx /usr/bin
COPY --from=builder /usr/src/zksync/docker/external-node/entrypoint.sh /usr/bin
COPY contracts/system-contracts/zkout/ /contracts/system-contracts/zkout/
COPY contracts/l1-contracts/out/ /contracts/l1-contracts/out/
COPY contracts/l1-contracts/zkout/ /contracts/l1-contracts/zkout/
COPY contracts/l2-contracts/zkout/ /contracts/l2-contracts/zkout/
COPY etc/tokens/ /etc/tokens/
COPY etc/ERC20/ /etc/ERC20/
COPY etc/multivm_bootloaders/ /etc/multivm_bootloaders/
COPY core/lib/dal/migrations/ /migrations

RUN chmod +x /usr/bin/entrypoint.sh

ENTRYPOINT [ "sh", "/usr/bin/entrypoint.sh"]
