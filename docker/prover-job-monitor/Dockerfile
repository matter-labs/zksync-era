FROM ghcr.io/matter-labs/zksync-build-base:latest AS builder

ARG DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/zksync
COPY . .

RUN cd prover && CARGO_HOME=/usr/src/zksync/cargo cargo build --locked --release --bin zksync_prover_job_monitor

FROM ghcr.io/matter-labs/zksync-runtime-base:latest

COPY --from=builder /usr/src/zksync/prover/target/release/zksync_prover_job_monitor /usr/bin/

ENTRYPOINT ["/usr/bin/zksync_prover_job_monitor"]
