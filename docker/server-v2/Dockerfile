# Will work locally only after prior contracts build
# syntax=docker/dockerfile:experimental
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

RUN cargo build --manifest-path ./core/Cargo.toml --release --features=rocksdb/io-uring --bin zksync_server --bin block_reverter --bin merkle_tree_consistency_checker

FROM ghcr.io/matter-labs/zksync-runtime-base:latest

RUN apt-get update && apt-get install -y liburing-dev && \
    rm -rf /var/lib/apt/lists/*

EXPOSE 3000
EXPOSE 3031
EXPOSE 3030

COPY --from=builder /usr/src/zksync/core/target/release/zksync_server /usr/bin
COPY --from=builder /usr/src/zksync/core/target/release/block_reverter /usr/bin
COPY --from=builder /usr/src/zksync/core/target/release/merkle_tree_consistency_checker /usr/bin
COPY contracts/system-contracts/zkout/ /contracts/system-contracts/zkout/
COPY contracts/l1-contracts/out/ /contracts/l1-contracts/out/
COPY contracts/l1-contracts/zkout/ /contracts/l1-contracts/zkout/
COPY contracts/l2-contracts/zkout/ /contracts/l2-contracts/zkout/
COPY etc/tokens/ /etc/tokens/
COPY etc/ERC20/ /etc/ERC20/
COPY etc/multivm_bootloaders/ /etc/multivm_bootloaders/
COPY core/lib/dal/migrations/ /migrations
COPY prover/data/keys/fflonk_verification_snark_key.json /keys/fflonk_verification_snark_key.json

ENTRYPOINT ["zksync_server"]
