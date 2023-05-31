# syntax=docker/dockerfile:experimental
FROM matterlabs/llvm_runner:latest as builder

WORKDIR /usr/src/zksync
COPY . .
#Needed to get zkEVM package from github link with auth embedded
# Doesn't expected to work local
RUN CARGO_HOME=./cargo cargo build --release

FROM matterlabs/zksolc:v1.2.0 as zksolc-v1-2-0

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates wget && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /etc/zksolc-bin/v1.3.0 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.0 \
    && cp zksolc-linux-amd64-musl-v1.3.0 /etc/zksolc-bin/v1.3.0/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.0/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.1 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.1 \
    && cp zksolc-linux-amd64-musl-v1.3.1 /etc/zksolc-bin/v1.3.1/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.1/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.2 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.2 \
    && cp zksolc-linux-amd64-musl-v1.3.2 /etc/zksolc-bin/v1.3.2/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.2/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.3 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.3 \
    && cp zksolc-linux-amd64-musl-v1.3.3 /etc/zksolc-bin/v1.3.3/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.3/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.4 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.4 \
    && cp zksolc-linux-amd64-musl-v1.3.4 /etc/zksolc-bin/v1.3.4/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.4/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.5 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.5 \
    && cp zksolc-linux-amd64-musl-v1.3.5 /etc/zksolc-bin/v1.3.5/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.5/zksolc


RUN mkdir -p /etc/zksolc-bin/v1.3.6 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.6 \
    && cp zksolc-linux-amd64-musl-v1.3.6 /etc/zksolc-bin/v1.3.6/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.6/zksolc


RUN mkdir -p /etc/zksolc-bin/v1.3.7 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.7 \
    && cp zksolc-linux-amd64-musl-v1.3.7 /etc/zksolc-bin/v1.3.7/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.7/zksolc

RUN mkdir -p /etc/zksolc-bin/v1.3.8 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.8 \
    && cp zksolc-linux-amd64-musl-v1.3.8 /etc/zksolc-bin/v1.3.8/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.8/zksolc

RUN mkdir -p /etc/zksolc-bin/v1.3.9 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.9 \
    && cp zksolc-linux-amd64-musl-v1.3.9 /etc/zksolc-bin/v1.3.9/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.9/zksolc

RUN mkdir -p /etc/zksolc-bin/v1.3.10 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.10 \
    && cp zksolc-linux-amd64-musl-v1.3.10 /etc/zksolc-bin/v1.3.10/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.10/zksolc

COPY docker/contract-verifier/install-all-solc.sh install-all-solc.sh
RUN sh ./install-all-solc.sh

COPY --from=builder /usr/src/zksync/target/release/zksync_contract_verifier /usr/bin/
COPY etc/system-contracts/bootloader/build/artifacts/ /etc/system-contracts/bootloader/build/artifacts/
COPY etc/system-contracts/artifacts-zk /etc/system-contracts/artifacts-zk
# CMD tail -f /dev/null
ENTRYPOINT ["zksync_contract_verifier"]
