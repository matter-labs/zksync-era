FROM ghcr.io/matter-labs/zksync-build-base:latest AS builder

ARG DEBIAN_FRONTEND=noninteractive
ARG RUST_FLAGS=""
ENV RUSTFLAGS=${RUST_FLAGS}
ENV ZKSYNC_USE_CUDA_STUBS=true

WORKDIR /usr/src/zksync
COPY . .

RUN cd prover && CARGO_HOME=/usr/src/zksync/cargo cargo build --locked --release --bin zksync_witness_generator

FROM ghcr.io/matter-labs/zksync-runtime-base:latest

COPY prover/data/keys/ /prover/data/keys/

COPY --from=builder /usr/src/zksync/prover/target/release/zksync_witness_generator /usr/bin/

ENTRYPOINT ["zksync_witness_generator"]
