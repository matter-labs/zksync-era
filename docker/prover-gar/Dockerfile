# syntax=docker/dockerfile:1
ARG PROVER_IMAGE
FROM us-docker.pkg.dev/matterlabs-infra/matterlabs-docker/prover-v2:2.0-$PROVER_IMAGE as prover

FROM nvidia/cuda:11.8.0-runtime-ubuntu22.04 as app

# HACK copying to root is the only way to make Docker layer caching work for these files for some reason
COPY *.bin /
COPY setup_2\^26.key /setup_2\^26.key

RUN apt-get update && apt-get install -y libpq5 ca-certificates openssl && rm -rf /var/lib/apt/lists/*

COPY --from=prover etc/system-contracts/bootloader/build/artifacts/ /etc/system-contracts/bootloader/build/artifacts/
COPY --from=prover etc/system-contracts/artifacts-zk /etc/system-contracts/artifacts-zk
COPY --from=prover contracts/ethereum/artifacts/ /contracts/ethereum/artifacts/
COPY --from=prover contracts/zksync/artifacts-zk/ /contracts/zksync/artifacts-zk/
COPY --from=prover core/bin/verification_key_generator_and_server/data/ /core/bin/verification_key_generator_and_server/data/
COPY --from=prover /usr/bin/zksync_prover /usr/bin/

ENTRYPOINT ["zksync_prover"]
