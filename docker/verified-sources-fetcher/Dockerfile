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

RUN cargo build --manifest-path ./core/Cargo.toml --release --bin verified_sources_fetcher

FROM ghcr.io/matter-labs/zksync-runtime-base:latest

RUN apt-get update && apt-get install -y apt-transport-https gnupg git && rm -rf /var/lib/apt/lists/*

RUN curl https://packages.cloud.google.com/apt/doc/apt-key.gpg | gpg --dearmor -o /usr/share/keyrings/cloud.google.gpg
RUN echo "deb [signed-by=/usr/share/keyrings/cloud.google.gpg] https://packages.cloud.google.com/apt cloud-sdk main" | tee -a /etc/apt/sources.list.d/google-cloud-sdk.list
RUN apt-get update && apt-get install -y google-cloud-cli && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/zksync/core/target/release/verified_sources_fetcher /usr/bin/

ENTRYPOINT ["verified_sources_fetcher"]
