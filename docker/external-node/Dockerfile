# Will work locally only after prior contracts build

FROM ghcr.io/matter-labs/zksync-build-base:latest AS builder

# set of args for use of sccache
ARG SCCACHE_GCS_BUCKET=""
ARG SCCACHE_GCS_SERVICE_ACCOUNT=""
ARG SCCACHE_GCS_RW_MODE=""
ARG RUSTC_WRAPPER=""

ENV SCCACHE_GCS_BUCKET=${SCCACHE_GCS_BUCKET}
ENV SCCACHE_GCS_SERVICE_ACCOUNT=${SCCACHE_GCS_SERVICE_ACCOUNT}
ENV SCCACHE_GCS_RW_MODE=${SCCACHE_GCS_RW_MODE}
ENV RUSTC_WRAPPER=${RUSTC_WRAPPER}

WORKDIR /usr/src/zksync
COPY . .
RUN cargo build --manifest-path ./core/Cargo.toml --release --bin zksync_external_node --bin block_reverter

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
