# syntax=docker/dockerfile:experimental
FROM ghcr.io/matter-labs/zksync-build-base:latest AS builder

ARG CUDA_ARCH=89
ENV CUDAARCHS=${CUDA_ARCH}
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

RUN cargo build --manifest-path ./core/Cargo.toml --release --bin zksync_contract_verifier

FROM ghcr.io/matter-labs/zksync-runtime-base:latest

RUN apt-get update && apt-get install -y wget python3 jq && rm -rf /var/lib/apt/lists/*

# install zksolc 1.3.x
RUN skip_versions="v1.3.12 v1.3.15 v1.3.20" && \
    for VERSION in $(seq -f "v1.3.%g" 0 23); do \
    if echo " $skip_versions " | grep -q -w " $VERSION "; then \
    continue; \
    fi; \
    mkdir -p /etc/zksolc-bin/$VERSION && \
    wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-$VERSION -O /etc/zksolc-bin/$VERSION/zksolc && \
    chmod +x /etc/zksolc-bin/$VERSION/zksolc; \
    done

# install zksolc 1.4.x
RUN for VERSION in $(seq -f "v1.4.%g" 0 1); do \
    mkdir -p /etc/zksolc-bin/$VERSION && \
    wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-$VERSION -O /etc/zksolc-bin/$VERSION/zksolc && \
    chmod +x /etc/zksolc-bin/$VERSION/zksolc; \
    done

# install zksolc vm1.5.0
RUN mkdir -p /etc/zksolc-bin/vm-1.5.0-a167aa3 && \
    wget https://github.com/matter-labs/era-compiler-solidity/releases/download/prerelease-a167aa3-code4rena/zksolc-linux-amd64-musl-v1.5.0 -O /etc/zksolc-bin/vm-1.5.0-a167aa3/zksolc && \
    chmod +x /etc/zksolc-bin/vm-1.5.0-a167aa3/zksolc

# install zksolc 1.5.x
RUN for VERSION in $(seq -f "v1.5.%g" 0 7); do \
    mkdir -p /etc/zksolc-bin/$VERSION && \
    wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-$VERSION -O /etc/zksolc-bin/$VERSION/zksolc && \
    chmod +x /etc/zksolc-bin/$VERSION/zksolc; \
    done

# install zkvyper 1.3.x
RUN for VERSION in $(seq -f "v1.3.%g" 9 17); do \
    mkdir -p /etc/zkvyper-bin/$VERSION && \
    wget https://github.com/matter-labs/zkvyper-bin/raw/main/linux-amd64/zkvyper-linux-amd64-musl-$VERSION -O /etc/zkvyper-bin/$VERSION/zkvyper && \
    chmod +x /etc/zkvyper-bin/$VERSION/zkvyper; \
    done

# install zkvyper 1.4.x
RUN for VERSION in $(seq -f "v1.4.%g" 0 1); do \
    mkdir -p /etc/zkvyper-bin/$VERSION && \
    wget https://github.com/matter-labs/zkvyper-bin/raw/main/linux-amd64/zkvyper-linux-amd64-musl-$VERSION -O /etc/zkvyper-bin/$VERSION/zkvyper && \
    chmod +x /etc/zkvyper-bin/$VERSION/zkvyper; \
    done

# install zkvyper 1.5.x
RUN for VERSION in $(seq -f "v1.5.%g" 0 7); do \
    mkdir -p /etc/zkvyper-bin/$VERSION && \
    wget https://github.com/matter-labs/zkvyper-bin/raw/main/linux-amd64/zkvyper-linux-amd64-musl-$VERSION -O /etc/zkvyper-bin/$VERSION/zkvyper && \
    chmod +x /etc/zkvyper-bin/$VERSION/zkvyper; \
    done

# install solc
COPY docker/contract-verifier/install-all-solc.sh install-all-solc.sh
COPY bin/run_retried run_retried
RUN bash ./install-all-solc.sh

# install vyper
RUN mkdir -p /etc/vyper-bin/0.3.3 \
    && wget -O vyper0.3.3 https://github.com/vyperlang/vyper/releases/download/v0.3.3/vyper.0.3.3%2Bcommit.48e326f0.linux \
    && mv vyper0.3.3 /etc/vyper-bin/0.3.3/vyper \
    && chmod +x /etc/vyper-bin/0.3.3/vyper
RUN mkdir -p /etc/vyper-bin/0.3.9 \
    && wget -O vyper0.3.9 https://github.com/vyperlang/vyper/releases/download/v0.3.9/vyper.0.3.9%2Bcommit.66b96705.linux \
    && mv vyper0.3.9 /etc/vyper-bin/0.3.9/vyper \
    && chmod +x /etc/vyper-bin/0.3.9/vyper
RUN mkdir -p /etc/vyper-bin/0.3.10 \
    && wget -O vyper0.3.10 https://github.com/vyperlang/vyper/releases/download/v0.3.10/vyper.0.3.10%2Bcommit.91361694.linux \
    && mv vyper0.3.10 /etc/vyper-bin/0.3.10/vyper \
    && chmod +x /etc/vyper-bin/0.3.10/vyper
RUN mkdir -p /etc/vyper-bin/0.4.0 \
    && wget -O vyper0.4.0 https://github.com/vyperlang/vyper/releases/download/v0.4.0/vyper.0.4.0+commit.e9db8d9f.linux \
    && mv vyper0.4.0 /etc/vyper-bin/0.4.0/vyper \
    && chmod +x /etc/vyper-bin/0.4.0/vyper

COPY --from=builder /usr/src/zksync/core/target/release/zksync_contract_verifier /usr/bin/
COPY contracts/system-contracts/zkout/ /contracts/system-contracts/zkout/

# CMD tail -f /dev/null
ENTRYPOINT ["zksync_contract_verifier"]
