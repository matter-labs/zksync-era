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

FROM ghcr.io/matter-labs/zksync-runtime-base:latest AS compiler-fetcher

WORKDIR /
RUN apt-get update && apt-get install -y --no-install-recommends wget jq ca-certificates && rm -rf /var/lib/apt/lists/*
COPY bin/run_retried run_retried

# Install the zksolc releases exposed by `contract_verifier::public_api` from immutable URLs.
RUN set -eu; \
    for VERSION_AND_SHA in \
      "v1.5.15:b52df6ae7e64903deb4972f8a41c9c666a7ce1dfdce38680f387df01afe51b4b" \
      "v1.5.16:f900698f8b4d73ad3a811c40358e9ae6e3590d368e0f583200f3d310979bf5ce" \
      "v1.5.17:3bda51f97a3087e7d959c05aa992f7d00f98acedfdc9caede64b334aaa76181f"; do \
      VERSION="${VERSION_AND_SHA%%:*}"; \
      SHA256="${VERSION_AND_SHA#*:}"; \
      mkdir -p "/etc/zksolc-bin/$VERSION"; \
      ./run_retried wget "https://github.com/matter-labs/era-compiler-solidity/releases/download/${VERSION#v}/zksolc-linux-amd64-musl-$VERSION" \
        -O "/etc/zksolc-bin/$VERSION/zksolc"; \
      echo "$SHA256  /etc/zksolc-bin/$VERSION/zksolc" | sha256sum -c -; \
      chmod +x "/etc/zksolc-bin/$VERSION/zksolc"; \
    done

# install solc
COPY docker/contract-verifier/install-all-solc.sh install-all-solc.sh
RUN bash ./install-all-solc.sh

FROM ghcr.io/matter-labs/zksync-runtime-base:latest

COPY --from=builder /usr/src/zksync/core/target/release/zksync_contract_verifier /usr/bin/
COPY --from=compiler-fetcher /etc/solc-bin/ /etc/solc-bin/
COPY --from=compiler-fetcher /etc/zksolc-bin/ /etc/zksolc-bin/
COPY contracts/system-contracts/zkout/ /contracts/system-contracts/zkout/

# CMD tail -f /dev/null
RUN useradd --system --uid 1000 --no-create-home contract-verifier
USER contract-verifier
ENTRYPOINT ["zksync_contract_verifier"]
