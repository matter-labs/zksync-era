# ===========================
# Stage 1: Builder
# ===========================
ARG BASE_VERSION=latest2.0
FROM matterlabs/server-v2:${BASE_VERSION} AS builder

# Set work directory
WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential make bash git openssl libssl-dev gcc g++ curl pkg-config \
    software-properties-common jq wget clang llvm-dev libclang-dev binutils \
    postgresql-client ca-certificates gnupg \
    && rm -rf /var/lib/apt/lists/*

# Install Rust and required toolchain
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y \
    && rustup install nightly-2024-08-01-x86_64-unknown-linux-gnu \
    && rustup default nightly-2024-08-01-x86_64-unknown-linux-gnu

# Install Cargo tools
RUN cargo install sqlx-cli --version 0.8.0

# Copy Cargo files BEFORE running `cargo clean`
COPY core/Cargo.toml core/Cargo.lock prover/Cargo.toml prover/Cargo.lock zkstack_cli/Cargo.toml zkstack_cli/Cargo.lock /app/

# Copy source files needed for compilation
COPY zkstack_cli /app/zkstack_cli
COPY rust-toolchain /app/
COPY core /app/core

# Avoid CPU-specific instructions
ENV RUSTFLAGS="-C target-cpu=generic"

# Set stable toolchain (avoid nightly quirks)
RUN rustup default stable

# Build zkstackup binary
RUN cargo install --git https://github.com/matter-labs/zksync-era.git --rev 634ff6a62e74b68ace1fd5dec40a5d77f77dcebf --locked zkstack --force --root /usr/local --features gateway

RUN rustup default nightly-2024-08-01-x86_64-unknown-linux-gnu

RUN which zkstack

# Build Foundry for zkSync
RUN mkdir ./foundry-zksync \
    && curl -LO https://github.com/matter-labs/foundry-zksync/releases/download/nightly-27360d4c8d12beddbb730dae07ad33a206b38f4b/foundry_nightly_linux_amd64.tar.gz \
    && tar zxf foundry_nightly_linux_amd64.tar.gz -C ./foundry-zksync \
    && chmod +x ./foundry-zksync/forge ./foundry-zksync/cast \
    && rm -rf foundry_nightly_linux_amd64.tar.gz

# ===========================
# Stage 2: Runtime Image (Slim & Optimized)
# ===========================
FROM matterlabs/server-v2:${BASE_VERSION} AS runtime

# Set work directory
WORKDIR /

# Install PostgreSQL client (psql) in final image
RUN apt-get update && apt-get install -y --no-install-recommends postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# Install required runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential make bash git openssl libssl-dev gcc g++ curl pkg-config \
    software-properties-common jq wget clang llvm-dev libclang-dev binutils \
    postgresql-client ca-certificates gnupg \
    libpq5 nodejs npm docker.io git \
    && rm -rf /var/lib/apt/lists/*

# Install Yarn
RUN npm install -g yarn

# Set environment variables
ENV PATH="/usr/local/cargo/bin:/usr/local/bin:/foundry-zksync:$PATH" \
    ZKSYNC_HOME=/ \
    ZKSYNC_LOCAL_SETUP=true \
    ZKSYNC_ACTION=dont_ask \
    IN_DOCKER=1 \
    LIBCLANG_PATH=/usr/lib/llvm-14/lib

# Copy Rust executables from builder stage
COPY --from=builder /usr/local/cargo/bin/cargo /usr/local/bin/cargo
COPY --from=builder /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx
COPY --from=builder /usr/local/cargo/bin/rustc /usr/local/bin/rustc
COPY --from=builder /usr/local/cargo/bin/rustup /usr/local/bin/rustup

# Copy compiled Rust & Foundry binaries from builder
COPY --from=builder /usr/local/bin/zkstack /usr/local/bin/zkstack
# COPY --from=builder /app/core/target/release/zksync_server /usr/local/bin/zksync_server
COPY --from=builder /app/foundry-zksync/forge /usr/local/bin/forge
COPY --from=builder /app/foundry-zksync/cast /usr/local/bin/cast

# Ensure binaries are executable
RUN chmod +x /usr/local/bin/*
RUN chmod +x /usr/local/bin/zkstack /usr/local/bin/forge /usr/local/bin/cast

# Copy required project files
COPY infrastructure/local-setup-preparation /infrastructure/local-setup-preparation
COPY etc/env /etc/env
COPY etc/test_config /etc/test_config
COPY contracts /contracts/
COPY core/ /core
COPY chains/era /chains/era
COPY package.json /
COPY docker-compose.yml /docker-compose.yml
COPY ZkStack.yaml /ZkStack.yaml

# Copy entrypoint script
COPY ./docker/local-node/entrypoint.sh /usr/bin/

# Copy Rust dependencies
COPY rust-toolchain /
COPY core/lib/dal core/lib/dal
COPY prover/crates/lib/prover_dal prover/crates/lib/prover_dal

# Prepare directory structure
RUN mkdir -p /etc/env/l1-inits /etc/env/l2-inits

# Ensure TypeScript dependencies are installed
RUN yarn global add ts-node typescript

# Install and build Node.js dependencies (production-only)
RUN cd /contracts/da-contracts && NODE_ENV=production yarn install && yarn build:foundry && cd /
RUN cd /contracts/l1-contracts && NODE_ENV=production yarn install --frozen-lockfile --ignore-optional && yarn build:foundry && cd /
RUN cd /contracts/l2-contracts && NODE_ENV=production yarn install && yarn build:foundry && cd /
RUN cd /contracts/system-contracts && yarn install && yarn build:foundry && cd /

RUN apt-get update && apt-get install -y --no-install-recommends \
    lsb-release \
  && mkdir -p /etc/apt/keyrings \
  && curl -fsSL https://download.docker.com/linux/debian/gpg \
    | gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
  && echo \
    "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
    https://download.docker.com/linux/debian $(lsb_release -cs) stable" \
    | tee /etc/apt/sources.list.d/docker.list \
  && apt-get update \
  && apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin \
  && rm -rf /var/lib/apt/lists/*

# Install cmake & other dependencies
RUN apt-get update && apt-get install -y cmake build-essential clang pkg-config libssl-dev

# Build zkstack server
# ENV RUSTFLAGS="-C target-feature=-avx512f,-avx512vl"
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
ENV CARGO_PROFILE_RELEASE_LTO="fat"
ENV RUSTFLAGS="-C target-cpu=generic -C target-feature=-avx512f,-avx512vl,-avx2,-sse4.2"
ENV CARGO_INCREMENTAL=0

RUN zkstack server build && cp /core/target/release/zksync_server /zksync_server

# Clean Yarn cache
RUN rm -rf /usr/local/share/.cache/yarn /core/target/

# Set project & workspace variables
ENV COMPOSE_PROJECT_NAME=my_project_name
ENV GITHUB_WORKSPACE=my_github_workspace

# Define entrypoint
ENTRYPOINT ["entrypoint.sh"]
