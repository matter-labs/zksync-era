ARG PROOF_COMPRESSOR_IMAGE
FROM us-docker.pkg.dev/matterlabs-infra/matterlabs-docker/proof-fri-gpu-compressor:2.0-$PROOF_COMPRESSOR_IMAGE as proof_fri_gpu
FROM nvidia/cuda:12.4.0-runtime-ubuntu22.04 as app

# HACK copying to root is the only way to make Docker layer caching work for these files for some reason
COPY *.bin /
COPY ./setup_compact.key /setup_compact.key

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

# copy finalization hints required for assembly generation
COPY --from=proof_fri_gpu /prover/data/keys/ /prover/data/keys/
COPY --from=proof_fri_gpu /usr/bin/zksync_proof_fri_compressor /usr/bin/

ENV COMPACT_CRS_FILE=/setup_compact.key

# Hack to make compressor work because it for very unknown reason shares config with prover which has setup-data in /setup-data directory now.
RUN ln -s / /setup-data

ENTRYPOINT ["zksync_proof_fri_compressor"]
