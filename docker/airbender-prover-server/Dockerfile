# syntax=docker/dockerfile:1
#
# GPU image for the Airbender prover server (FRI on GPU + SNARK wrapper).
#
# IMPORTANT: the build context for this Dockerfile is the `airbender_prover_server/`
# workspace directory, NOT the repo root — that workspace is self-contained and
# pulls `eravm-prover-host` / the guest verifier crates as git dependencies:
#
#   docker build -f docker/airbender-prover-server/Dockerfile airbender_prover_server
#
# ─── Build stage ─────────────────────────────────────────────────────────────
# CUDA 13.0.0 (GA) devel toolchain, kept in lockstep with the 13.0.0-runtime
# base below. The guest is NOT built here (committed app.bin is used), so we
# skip cargo-airbender and its guest-only extras.
FROM nvidia/cuda:13.0.0-devel-ubuntu22.04 AS builder

ARG RUST_TOOLCHAIN=nightly-2026-02-10
ARG CMAKE_VERSION=3.30.2

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:${PATH} \
    CARGO_TERM_COLOR=always

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        clang \
        curl \
        git \
        libssl-dev \
        lld \
        pkg-config \
        wget && \
    rm -rf /var/lib/apt/lists/*

# gpu_prover's CUDA build via the `cmake` crate needs newer CUDA language
# handling than Ubuntu 22.04's packaged CMake provides.
RUN curl -LO "https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/cmake-${CMAKE_VERSION}-linux-x86_64.sh" && \
    chmod +x "cmake-${CMAKE_VERSION}-linux-x86_64.sh" && \
    "./cmake-${CMAKE_VERSION}-linux-x86_64.sh" --skip-license --prefix=/usr/local && \
    rm "cmake-${CMAKE_VERSION}-linux-x86_64.sh"

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --no-modify-path --default-toolchain "${RUST_TOOLCHAIN}"

WORKDIR /workspace

# CUDA archs to build for. Tailored to prod hardware only: sm_80 (A100) and
# sm_120 (RTX PRO 6000 Blackwell).
ENV CUDAARCHS="80;120"

# Step 1: build era-bellman-cuda. `zksync-crypto-gpu`'s `gpu-ffi` build script
# reads `BELLMAN_CUDA_DIR` from the env when compiled with `--features
# gpu_snark`. Pinned to a specific commit so the Docker layer cache invalidates
# intentionally on upstream bumps. Sits above `COPY . .` so the slow cmake build
# caches across repo edits.
ARG BELLMAN_CUDA_COMMIT=d1fa8670ee84ec3477c6cc1c85a3554cfa5e0206
RUN git clone https://github.com/matter-labs/era-bellman-cuda.git /workspace/era-bellman-cuda \
    && git -C /workspace/era-bellman-cuda checkout "${BELLMAN_CUDA_COMMIT}" \
    && cmake -B /workspace/era-bellman-cuda/build -S /workspace/era-bellman-cuda -DCMAKE_BUILD_TYPE=Release \
    && cmake --build /workspace/era-bellman-cuda/build -j"$(nproc)"
ENV BELLMAN_CUDA_DIR=/workspace/era-bellman-cuda

# Step 2: stage the bellman-cuda shared libraries the runtime needs.
RUN mkdir -p /bellman-cuda-libs \
    && find /workspace/era-bellman-cuda/build -type f \( -name '*.so*' -o -name '*.a' \) \
        -exec cp -v {} /bellman-cuda-libs/ \;

# Step 3: pre-fetch the bellman SNARK trusted setup so the runtime image ships
# with it already in place. GPU build uses `setup_compact.key`. Above `COPY . .`
# so repo edits don't re-fetch the setup file (~hundreds of MB).
ARG SNARK_TRUSTED_SETUP_URL="https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_compact.key"
RUN mkdir -p /setup \
    && curl --fail --location --retry 5 --retry-delay 5 \
        "${SNARK_TRUSTED_SETUP_URL}" -o /setup/setup.key

# Step 4: bring the workspace in. Everything below busts the layer cache on any
# source change.
COPY . .

# The guest program (`guest/dist/app/{app.bin,app.text}`) and verification keys
# (`vks/`) are NOT in the build context — they are gitignored and downloaded by
# the server crate's `build.rs` from the pinned `zksync_airbender_verifier`
# GitHub release during the `cargo build` below, so no guard/vendoring step is
# needed here. A failed download fails `cargo build` loudly.

# Step 5: build the server binary (FRI on GPU + SNARK wrapper on GPU). BuildKit
# cache mounts persist cargo's registry, git, and target dirs across builds.
# Cache mounts aren't part of the image layers, so we copy the binary out to
# `/out` within the same RUN — the runtime stage COPYs from there.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/workspace/target \
    cargo build --release --locked --features gpu_snark \
        -p eravm-prover-server \
    && mkdir -p /out \
    && cp /workspace/target/release/eravm-prover-server /out/

# ─── Runtime stage ────────────────────────────────────────────────────────────
FROM nvidia/cuda:13.0.0-runtime-ubuntu22.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /out/eravm-prover-server /usr/local/bin/eravm-prover-server
COPY --from=builder /workspace/guest/dist/app /guest-program
COPY --from=builder /workspace/vks /vks
COPY --from=builder /setup/setup.key /setup/setup.key
COPY --from=builder /bellman-cuda-libs /usr/local/lib/bellman-cuda

# `libbellman-cuda.so` etc. aren't in any default loader path; register them via
# ldconfig so the dynamic linker picks them up without a runtime override.
RUN echo "/usr/local/lib/bellman-cuda" > /etc/ld.so.conf.d/bellman-cuda.conf \
    && ldconfig

ENV PROVER_GUEST_DIST_DIR=/guest-program

# The vendored FRI and SNARK verification keys ship with the image. The server
# hard-fails at startup if either file is missing — it never derives a VK on
# the fly.
ENV FRI_VK=/vks/fri_vk.bin
ENV SNARK_VK=/vks/snark_vk.json

# Bellman SNARK trusted setup ships with the image.
ENV SNARK_TRUSTED_SETUP_FILE=/setup/setup.key

# Default stack size for Rust-spawned threads. The SNARK wrapper's recursion
# blows past Rust's 2 MB default.
ENV RUST_MIN_STACK=134217728

# Optional Prometheus metrics port
EXPOSE 3000

# SNARK wrapper recursion needs an unbounded stack — see README.md. Raise the
# soft RLIMIT_STACK before exec'ing; glibc seeds pthread stack sizes from
# RLIMIT_STACK, so this also reaches the prover's worker threads. The container
# runtime must permit it (`docker run --ulimit stack=-1`, or no explicit
# Kubernetes cap), otherwise the soft limit clamps to the hard limit and SNARK
# proving will still crash.
ENTRYPOINT ["/bin/sh", "-c", "ulimit -s unlimited; exec /usr/local/bin/eravm-prover-server \"$@\"", "--"]
