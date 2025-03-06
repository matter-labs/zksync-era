# ===========================
# Stage 1: Builder
# ===========================
ARG BASE_VERSION=latest2.0
FROM matterlabs/server-v2:${BASE_VERSION} AS builder

# Set work directory
WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential make bash git openssl libssl-dev gcc g++ curl pkg-config \
    software-properties-common jq wget clang llvm-dev libclang-dev \
    postgresql-client ca-certificates gnupg 

# Install Rust and required toolchain
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y \
    && rustup install nightly-2024-08-01-x86_64-unknown-linux-gnu \
    && rustup default nightly-2024-08-01-x86_64-unknown-linux-gnu

# Install Cargo tools
RUN cargo install sqlx-cli --version 0.8.0

# Copy source files needed for compilation
COPY zkstack_cli /app/zkstack_cli
COPY rust-toolchain /app/
COPY core /app/core

# Build zkstackup binary
RUN ./zkstack_cli/zkstackup/install -g --path ./zkstack_cli/zkstackup/zkstackup \
    && zkstackup -g --local --cargo-features gateway

# Build Foundry for zkSync
RUN mkdir ./foundry-zksync
RUN curl -LO https://github.com/matter-labs/foundry-zksync/releases/download/nightly-27360d4c8d12beddbb730dae07ad33a206b38f4b/foundry_nightly_linux_amd64.tar.gz
RUN tar zxf foundry_nightly_linux_amd64.tar.gz -C ./foundry-zksync
RUN chmod +x ./foundry-zksync/forge ./foundry-zksync/cast

# ===========================
# Stage 2: Runtime Image
# ===========================
ARG BASE_VERSION=latest2.0
FROM matterlabs/server-v2:${BASE_VERSION}

# Set work directory
WORKDIR /

# Install PostgreSQL client (psql) in final image
RUN apt-get update && apt-get install -y postgresql-client

# Install required runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates curl gnupg libpq5 \
    nodejs npm docker.io \
    && rm -rf /var/lib/apt/lists/*

# Install Yarn
RUN npm install -g yarn

# Set environment variables
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/local/bin:/foundry-zksync:$PATH \
    ZKSYNC_HOME=/ \
    ZKSYNC_LOCAL_SETUP=true \
    ZKSYNC_ACTION=dont_ask \
    IN_DOCKER=1 \
    LIBCLANG_PATH=/usr/lib/llvm-14/lib

# Copy Rust executables (without reinstalling Rust)
COPY --from=builder /usr/local/cargo/bin/cargo /usr/local/bin/cargo
COPY --from=builder /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx
COPY --from=builder /usr/local/cargo/bin/rustc /usr/local/bin/rustc
COPY --from=builder /usr/local/cargo/bin/rustup /usr/local/bin/rustup

# Copy compiled binaries from the builder stage
COPY --from=builder /usr/local/bin/zkstack /usr/local/bin/zkstack
COPY --from=builder /app/foundry-zksync/forge /usr/local/bin/forge
COPY --from=builder /app/foundry-zksync/cast /usr/local/bin/cast

RUN chmod +x /usr/local/bin/zkstack

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

# Copy Rust dependencies
COPY rust-toolchain /
COPY core/lib/dal core/lib/dal
COPY prover/crates/lib/prover_dal prover/crates/lib/prover_dal

# Prepare directory structure
RUN mkdir -p /etc/env/l1-inits /etc/env/l2-inits

# Install and build Node.js dependencies
RUN cd /contracts/da-contracts && yarn && yarn build:foundry && cd /
RUN cd /contracts/l1-contracts && yarn install --frozen-lockfile && yarn build:foundry && cd /
RUN cd /contracts/l2-contracts && yarn && yarn build:foundry && cd /
RUN cd /contracts/system-contracts && yarn && yarn build:foundry && cd /

ENV COMPOSE_PROJECT_NAME=my_project_name
ENV GITHUB_WORKSPACE=my_github_workspace

# Install Docker
RUN apt-get update && apt-get install -y docker.io

# Install required dependencies including Docker
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl gnupg lsb-release \
    && mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null \
    && apt-get update \
    && apt-get install -y docker-compose-plugin \
    && rm -rf /var/lib/apt/lists/*

# Define entrypoint
ENTRYPOINT ["entrypoint.sh"]