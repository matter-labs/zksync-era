# Image is always built from the server image to reuse the common parts
# This image is expected to be built locally beforehand (implemented in the `zk` tool)
ARG BASE_VERSION=latest2.0
FROM matterlabs/server-v2:${BASE_VERSION}

WORKDIR /

# Install required dependencies
RUN apt-get update; apt-get install -y make bash git openssl libssl-dev gcc g++ curl pkg-config software-properties-common jq wget vim-tiny
RUN apt-get install -y curl gnupg libpq5 ca-certificates postgresql-client && rm -rf /var/lib/apt/lists/*

# Install node and yarn
ENV NODE_MAJOR=18
RUN mkdir -p /etc/apt/keyrings && \
    wget -c -O - https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_$NODE_MAJOR.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && apt-get install nodejs -y && \
    npm install -g yarn

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# Install Rust and required toolchain
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y \
    && rustup install nightly-2024-08-01-x86_64-unknown-linux-gnu \
    && rustup default nightly-2024-08-01-x86_64-unknown-linux-gnu

RUN cargo install sqlx-cli --version 0.8.0

# Install additional dependencies
RUN apt-get update && apt-get install -y \
    clang llvm-dev libclang-dev \
    ca-certificates curl gnupg 
    # && curl -L https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/install-foundry-zksync | bash 

RUN mkdir ./foundry-zksync
RUN curl -LO https://github.com/matter-labs/foundry-zksync/releases/download/nightly-27360d4c8d12beddbb730dae07ad33a206b38f4b/foundry_nightly_linux_amd64.tar.gz
RUN tar zxf foundry_nightly_linux_amd64.tar.gz -C ./foundry-zksync
RUN chmod +x ./foundry-zksync/forge ./foundry-zksync/cast
ENV PATH="/foundry-zksync:$PATH"

ENV PATH="/usr/local/cargo/bin:/root/.foundry/bin:$PATH"

# Copy required packages while preserving the folders structure from the repo
# It's required because these packages use relative paths to the SDK

# Copy `local-setup-preparation` tool
COPY infrastructure/local-setup-preparation /infrastructure/local-setup-preparation
# Copy dev configs
COPY etc/env /etc/env
# Copy test configs (required to list rich accounts)
COPY etc/test_config /etc/test_config
# Copy all the L1 contracts so they can be deployed
COPY contracts /contracts/
COPY core/ /core
# Set `ZKSYNC_HOME` to the root (required for `zk` tool)
ENV ZKSYNC_HOME=/
# Set `LOCAL_SETUP` variable to modify `zk` tool behavior
ENV ZKSYNC_LOCAL_SETUP=true
# Disable all checks
ENV ZKSYNC_ACTION=dont_ask
# Env in Docker
ENV IN_DOCKER=1
# Build all the required TS packages

# Build `local-setup-preparation` tool
# RUN cd /infrastructure/local-setup-preparation && yarn && cd /
# Same for L1-da contracts
RUN cd /contracts/da-contracts && yarn && yarn build:foundry && cd /
# Build L1 contracts package (contracts themselves should be already built)
RUN cd /contracts/l1-contracts && yarn install --frozen-lockfile && yarn build:foundry && cd /
# Same for L2 contracts
RUN cd /contracts/l2-contracts && yarn && yarn build:foundry && cd /
# # Same for system contracts
RUN cd /contracts/system-contracts && yarn && yarn build:foundry && cd /

# Copy the ZK stack tool Library
COPY zkstack_cli /zkstack_cli
# Copy the configs for chain
COPY chains/era /chains/era
# Copy package json (which gives us yarn workspace - and makes commands more similar to what we normally run)
COPY package.json /
# Copy rust toolchain (so the compilation can be done with correct nightly)
COPY rust-toolchain /

# Copy DAL - needed to setup database schemas.
COPY core/lib/dal core/lib/dal
COPY prover/crates/lib/prover_dal prover/crates/lib/prover_dal

RUN mkdir /etc/env/l1-inits && mkdir /etc/env/l2-inits

# setup entrypoint script
COPY ./docker/local-node/entrypoint.sh /usr/bin/

COPY ./docker-compose.yml /docker-compose.yml
COPY ./ZkStack.yaml /ZkStack.yaml

# Install Docker
RUN apt-get update && apt-get install -y docker.io

# Configure Docker repository
# RUN apt-get update && \
#     apt-get install -y ca-certificates curl gnupg && \
#     mkdir -m 0755 /etc/apt/keyrings && \
#     curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg && \
#     echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null && \
#     apt-get update && \
#     apt-get install -y docker-compose-plugin && \
#     \
#     # Install Foundry for zkSync
#     curl -L https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/install-foundry-zksync | bash

# Persist environment variables
# ENV PATH=$HOME/.foundry/bin:$PATH
# ENV LIBCLANG_PATH=/usr/lib/llvm-14/lib
ENV COMPOSE_PROJECT_NAME=my_project_name
ENV GITHUB_WORKSPACE=my_github_workspace

# Install required dependencies including Docker
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl gnupg lsb-release \
    && mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null \
    && apt-get update \
    && apt-get install -y docker-compose-plugin \
    && rm -rf /var/lib/apt/lists/*

# Set persistent environment variables
ENV LIBCLANG_PATH=/usr/lib/llvm-14/lib

RUN ./zkstack_cli/zkstackup/install -g --path ./zkstack_cli/zkstackup/zkstackup \
    && zkstackup -g --local --cargo-features gateway

# CMD ["/bin/bash"]

ENTRYPOINT ["entrypoint.sh"]
