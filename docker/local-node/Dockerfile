# Image is always built from the server image to reuse the common parts
# This image is expected to be built locally beforehand (implemented in the `zk` tool)
FROM matterlabs/server-v2:latest2.0

WORKDIR /

# Install required dependencies
RUN apt-get update; apt-get install -y make bash git openssl libssl-dev gcc g++ curl pkg-config software-properties-common jq wget
RUN apt-get install -y curl gnupg libpq5 ca-certificates postgresql-client && rm -rf /var/lib/apt/lists/*

# Install node and yarn
ENV NODE_MAJOR=18
RUN mkdir -p /etc/apt/keyrings && \
    wget -c -O - https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_$NODE_MAJOR.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && apt-get install nodejs npm -y && \
    npm install -g yarn && npm install -g cspell && npm install -g markdown-link-check

# Copy compiler (both solc and zksolc) binaries
# Obtain `solc` 0.8.12.
RUN wget https://github.com/ethereum/solc-bin/raw/gh-pages/linux-amd64/solc-linux-amd64-v0.8.12%2Bcommit.f00d7308 \
    && mv solc-linux-amd64-v0.8.12+commit.f00d7308 /usr/bin/solc \
    && chmod +x /usr/bin/solc

# Copy required packages while preserving the folders structure from the repo
# It's required because these packages use relative paths to the SDK
# Copy `zk` tool
COPY infrastructure/zk /infrastructure/zk
# Copy `local-setup-preparation` tool
COPY infrastructure/local-setup-preparation /infrastructure/local-setup-preparation
# Copy migrations
COPY core/lib/dal/migrations /migrations
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

# setup entrypoint script
COPY ./docker/local-node/entrypoint.sh /usr/bin/
ENTRYPOINT ["entrypoint.sh"]
