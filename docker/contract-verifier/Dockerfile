# syntax=docker/dockerfile:experimental
FROM matterlabs/llvm_runner:latest as builder

WORKDIR /usr/src/zksync
COPY . .
#Needed to get zkEVM package from github link with auth embedded
# Doesn't expected to work local
RUN CARGO_HOME=./cargo cargo build --release

FROM matterlabs/zksolc:v1.2.0 as zksolc-v1-2-0

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates wget && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /etc/zksolc-bin/v1.2.0 && cp /usr/local/bin/zksolc /etc/zksolc-bin/v1.2.0/
RUN mkdir -p /etc/zksolc-bin/v1.2.1 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.2.1 \
    && cp zksolc-linux-amd64-musl-v1.2.1 /etc/zksolc-bin/v1.2.1/zksolc \
    && chmod +x /etc/zksolc-bin/v1.2.1/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.2.2 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.2.2 \
    && cp zksolc-linux-amd64-musl-v1.2.2 /etc/zksolc-bin/v1.2.2/zksolc \
    && chmod +x /etc/zksolc-bin/v1.2.2/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.2.3 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.2.3 \
    && cp zksolc-linux-amd64-musl-v1.2.3 /etc/zksolc-bin/v1.2.3/zksolc \
    && chmod +x /etc/zksolc-bin/v1.2.3/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.0 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.0 \
    && cp zksolc-linux-amd64-musl-v1.3.0 /etc/zksolc-bin/v1.3.0/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.0/zksolc
RUN mkdir -p /etc/zksolc-bin/v1.3.1 \
    && wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.1 \
    && cp zksolc-linux-amd64-musl-v1.3.1 /etc/zksolc-bin/v1.3.1/zksolc \
    && chmod +x /etc/zksolc-bin/v1.3.1/zksolc

COPY docker/contract-verifier/install-all-solc.sh install-all-solc.sh
RUN sh ./install-all-solc.sh

COPY --from=builder /usr/src/zksync/target/release/zksync_contract_verifier /usr/bin/
COPY etc/system-contracts/bootloader/build/artifacts/ /etc/system-contracts/bootloader/build/artifacts/
COPY etc/system-contracts/artifacts-zk /etc/system-contracts/artifacts-zk
# CMD tail -f /dev/null
ENTRYPOINT ["zksync_contract_verifier"]
