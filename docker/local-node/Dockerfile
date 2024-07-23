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

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y && \
    rustup install stable

RUN cargo install sqlx-cli --version 0.7.3

# Copy required packages while preserving the folders structure from the repo
# It's required because these packages use relative paths to the SDK
# Copy `zk` tool
COPY infrastructure/zk /infrastructure/zk
# Copy `local-setup-preparation` tool
COPY infrastructure/local-setup-preparation /infrastructure/local-setup-preparation
# Copy dev configs
COPY etc/env /etc/env
# Copy test configs (required to list rich accounts)
COPY etc/test_config /etc/test_config
# Copy all the L1 contracts so they can be deployed
COPY contracts /contracts/

# Set `ZKSYNC_HOME` to the root (required for `zk` tool)
ENV ZKSYNC_HOME=/
# Set `LOCAL_SETUP` variable to modify `zk` tool behavior
ENV ZKSYNC_LOCAL_SETUP=true
# Disable all checks
ENV ZKSYNC_ACTION=dont_ask

# Build all the required TS packages

# Build `zk` tool
RUN cd /infrastructure/zk && yarn && yarn build && cd /
# Build `local-setup-preparation` tool
RUN cd /infrastructure/local-setup-preparation && yarn && cd /
# Build L1 contracts package (contracts themselves should be already built)
RUN cd /contracts/l1-contracts && yarn && cd /
# Same for L2 contracts
RUN cd /contracts/l2-contracts && yarn && cd /

# Copy the ZK tool binary
COPY bin/zk /bin/zk
# Copy package json (which gives us yarn workspace - and makes commands more similar to what we normally run)
COPY package.json /

# Copy DAL - needed to setup database schemas.
COPY core/lib/dal core/lib/dal
COPY prover/crates/lib/prover_dal prover/crates/lib/prover_dal

RUN mkdir /etc/env/l1-inits && mkdir /etc/env/l2-inits

# setup entrypoint script
COPY ./docker/local-node/entrypoint.sh /usr/bin/

ENTRYPOINT ["entrypoint.sh"]
