# syntax=docker/dockerfile:experimental
FROM rust:1.67-bookworm as builder
RUN apt-get update && apt-get install -y clang && rm -rf /var/lib/apt/lists/*
WORKDIR /usr/src/zksync
COPY . .
#Needed to get zkEVM package from github link with auth embedded
# Doesn't expected to work local
RUN CARGO_HOME=./cargo cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y curl libpq5 ca-certificates wget python3 && rm -rf /var/lib/apt/lists/*

# install zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.0 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.0 \
    && mv zksolc-linux-amd64-musl-v1.3.0 /etc/zksolc-bin/v1.3.0/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.0/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.1 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.1 \
    && mv zksolc-linux-amd64-musl-v1.3.1 /etc/zksolc-bin/v1.3.1/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.1/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.2 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.2 \
    && mv zksolc-linux-amd64-musl-v1.3.2 /etc/zksolc-bin/v1.3.2/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.2/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.3 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.3 \
    && mv zksolc-linux-amd64-musl-v1.3.3 /etc/zksolc-bin/v1.3.3/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.3/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.4 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.4 \
    && mv zksolc-linux-amd64-musl-v1.3.4 /etc/zksolc-bin/v1.3.4/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.4/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.5 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.5 \
    && mv zksolc-linux-amd64-musl-v1.3.5 /etc/zksolc-bin/v1.3.5/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.5/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.6 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.6 \
    && mv zksolc-linux-amd64-musl-v1.3.6 /etc/zksolc-bin/v1.3.6/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.6/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.7 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.7 \
    && mv zksolc-linux-amd64-musl-v1.3.7 /etc/zksolc-bin/v1.3.7/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.7/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.8 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.8 \
    && mv zksolc-linux-amd64-musl-v1.3.8 /etc/zksolc-bin/v1.3.8/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.8/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.9 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.9 \
    && mv zksolc-linux-amd64-musl-v1.3.9 /etc/zksolc-bin/v1.3.9/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.9/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.10 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.10 \
    && mv zksolc-linux-amd64-musl-v1.3.10 /etc/zksolc-bin/v1.3.10/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.10/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.11 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.11 \
    && mv zksolc-linux-amd64-musl-v1.3.11 /etc/zksolc-bin/v1.3.11/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.11/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.13 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.13 \
    && mv zksolc-linux-amd64-musl-v1.3.13 /etc/zksolc-bin/v1.3.13/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.13/zksolc

# install zkvyper
RUN mkdir -p /etc/zkvyper-bin/v1.3.9 \
    && wget https://github.com/matter-labs/zkvyper-bin/raw/main/linux-amd64/zkvyper-linux-amd64-musl-v1.3.9 \
    && mv zkvyper-linux-amd64-musl-v1.3.9 /etc/zkvyper-bin/v1.3.9/zkvyper \
    && chmod +x /etc/zkvyper-bin/v1.3.9/zkvyper

# install solc
COPY docker/contract-verifier/install-all-solc.sh install-all-solc.sh
RUN sh ./install-all-solc.sh

# install vyper
RUN mkdir -p /etc/vyper-bin/0.3.3 \
    && wget -O vyper0.3.3 https://github.com/vyperlang/vyper/releases/download/v0.3.3/vyper.0.3.3%2Bcommit.48e326f0.linux \
    && mv vyper0.3.3 /etc/vyper-bin/0.3.3/vyper \
    && chmod +x /etc/vyper-bin/0.3.3/vyper
RUN mkdir -p /etc/vyper-bin/0.3.9 \
    && wget -O vyper0.3.9 https://github.com/vyperlang/vyper/releases/download/v0.3.9/vyper.0.3.9%2Bcommit.66b96705.linux \
    && mv vyper0.3.9 /etc/vyper-bin/0.3.9/vyper \
    && chmod +x /etc/vyper-bin/0.3.9/vyper

COPY --from=builder /usr/src/zksync/target/release/zksync_contract_verifier /usr/bin/
COPY etc/system-contracts/bootloader/build/artifacts/ /etc/system-contracts/bootloader/build/artifacts/
COPY etc/system-contracts/artifacts-zk /etc/system-contracts/artifacts-zk
# CMD tail -f /dev/null
ENTRYPOINT ["zksync_contract_verifier"]
