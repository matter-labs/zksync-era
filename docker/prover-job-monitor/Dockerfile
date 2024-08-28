FROM matterlabs/zksync-build-base:latest as builder

ARG DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/zksync
COPY . .

RUN cd prover && cargo build --release --bin zksync_prover_job_monitor

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/zksync/prover/target/release/zksync_prover_job_monitor /usr/bin/

ENTRYPOINT ["/usr/bin/zksync_prover_job_monitor"]
