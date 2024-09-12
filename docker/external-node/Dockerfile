# Will work locally only after prior contracts build

FROM matterlabs/zksync-build-base:latest as builder

WORKDIR /usr/src/zksync
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/zksync/target/release/zksync_external_node /usr/bin
COPY --from=builder /usr/src/zksync/target/release/block_reverter /usr/bin
COPY --from=builder /usr/local/cargo/bin/sqlx /usr/bin
COPY --from=builder /usr/src/zksync/docker/external-node/entrypoint.sh /usr/bin
COPY contracts/system-contracts/bootloader/build/artifacts/ /contracts/system-contracts/bootloader/build/artifacts/
COPY contracts/system-contracts/contracts-preprocessed/artifacts/ /contracts/system-contracts/contracts-preprocessed/artifacts/
COPY contracts/system-contracts/contracts-preprocessed/precompiles/artifacts/ /contracts/system-contracts/contracts-preprocessed/precompiles/artifacts/
COPY contracts/system-contracts/artifacts-zk /contracts/system-contracts/artifacts-zk
COPY contracts/l1-contracts/out/ /contracts/l1-contracts/out/
# TODO Remove once we use foundry inside contracts repo
COPY contracts/l1-contracts/artifacts/ /contracts/l1-contracts/artifacts/
COPY contracts/l2-contracts/artifacts-zk/ /contracts/l2-contracts/artifacts-zk/
COPY etc/tokens/ /etc/tokens/
COPY etc/ERC20/ /etc/ERC20/
COPY etc/multivm_bootloaders/ /etc/multivm_bootloaders/
COPY core/lib/dal/migrations/ /migrations

RUN chmod +x /usr/bin/entrypoint.sh

ENTRYPOINT [ "sh", "/usr/bin/entrypoint.sh"]
