FROM ghcr.io/matter-labs/zksync-build-base:latest AS builder

ARG DEBIAN_FRONTEND=noninteractive

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

RUN cd prover && cargo build --release --bin zksync_prover_fri_gateway

FROM ghcr.io/matter-labs/zksync-runtime-base:latest

# copy VK required for proof wrapping
COPY prover/data/keys/ /prover/data/keys/

COPY --from=builder /usr/src/zksync/prover/target/release/zksync_prover_fri_gateway /usr/bin/

ENTRYPOINT ["zksync_prover_fri_gateway"]
