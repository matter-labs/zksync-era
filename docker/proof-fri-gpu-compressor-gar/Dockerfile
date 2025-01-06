ARG PROOF_FRI_GPU_IMAGE
FROM us-docker.pkg.dev/matterlabs-infra/matterlabs-docker/proof-fri-gpu-compressor:2.0-$PROOF_FRI_GPU_IMAGE as proof_fri_gpu
FROM nvidia/cuda:12.4.0-runtime-ubuntu22.04 as app

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY ./setup_fflonk_compact.key /setup_fflonk_compact.key

# copy finalization hints required for assembly generation
COPY --from=proof_fri_gpu /setup_2\^24.key /setup_2\^24.key
COPY --from=proof_fri_gpu /prover/data/keys/ /prover/data/keys/
COPY --from=proof_fri_gpu /usr/bin/zksync_proof_fri_compressor /usr/bin/

ENV CRS_FILE=/setup_2\^24.key
ENV COMPACT_CRS_FILE=/setup_fflonk_compact.key


ENTRYPOINT ["zksync_proof_fri_compressor"]
