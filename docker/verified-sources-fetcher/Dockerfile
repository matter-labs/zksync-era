FROM matterlabs/zksync-build-base:latest as builder

ARG DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/zksync
COPY . .

RUN cargo build --release --bin verified_sources_fetcher

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y apt-transport-https ca-certificates gnupg curl git && rm -rf /var/lib/apt/lists/*

RUN curl https://packages.cloud.google.com/apt/doc/apt-key.gpg | gpg --dearmor -o /usr/share/keyrings/cloud.google.gpg
RUN echo "deb [signed-by=/usr/share/keyrings/cloud.google.gpg] https://packages.cloud.google.com/apt cloud-sdk main" | tee -a /etc/apt/sources.list.d/google-cloud-sdk.list
RUN apt-get update && apt-get install -y google-cloud-cli && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/zksync/target/release/verified_sources_fetcher /usr/bin/

ENTRYPOINT ["verified_sources_fetcher"]
