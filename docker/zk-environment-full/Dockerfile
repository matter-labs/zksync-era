FROM matterlabs/zk-environment:latest2.0-lightweight

WORKDIR /usr/src

RUN git clone https://github.com/matter-labs/zksync-era

WORKDIR zksync-era

# get rid of already installed rust
RUN rustup uninstall stable

# git submodule
RUN git submodule init
RUN git submodule update

# rust
RUN SERVER_RUST_VERSION=$(cat rust-toolchain) && rustup install $SERVER_RUST_VERSION
RUN cargo fetch

RUN PROVER_RUST_VERSION=$(cat prover/rust-toolchain) && rustup install $PROVER_RUST_VERSION
RUN cd prover && cargo fetch

# yarn packages
RUN yarn install --frozen-lockfile

# as repository directory must be empty node_modules need to moved somewhere and then restored after git checkout
RUN mkdir ../node_modules_backup
ENV NODE_MODULES_BACKUP_DIR=/usr/src/node_modules_backup
RUN (for node_dir in . etc/contracts-test-data contracts; do mkdir -p ../node_modules_backup/$node_dir; mv $node_dir/node_modules ../node_modules_backup/$node_dir; done)

# Contracts compilers

# This ./cache/hardhat-nodejs is coming from the env-paths module
# that hardhat is using.
ENV COMPILER_DIR=/root/.cache/hardhat-nodejs/compilers-v2
RUN  mkdir -p $COMPILER_DIR/linux-amd64
RUN  mkdir -p $COMPILER_DIR/vyper/linux
RUN  mkdir -p $COMPILER_DIR/zksolc
RUN  mkdir -p $COMPILER_DIR/zkvyper

# Fetch latest compiler version
RUN wget -nv -O $COMPILER_DIR/zksolc/compilerVersionInfo.json  "https://raw.githubusercontent.com/matter-labs/zksolc-bin/main/version.json"


# These are the versions that we currently have in hardhat.config.ts in zksync-era and era-contracts.
# For now, if there is a new version of compiler, we'd have to modify this file.
# In the future, we should make it more automatic.
RUN (for ver in v1.3.18 v1.3.21 v1.4.0 v1.4.1; do wget -nv -O $COMPILER_DIR/zksolc/zksolc-$ver  https://raw.githubusercontent.com/matter-labs/zksolc-bin/main/linux-amd64/zksolc-linux-amd64-musl-$ver; done)

# Special pre-release 1.5.0 compiler.
# It can be removed once system-contracts/hardhatconfig.ts stops using it.
RUN wget -nv -O $COMPILER_DIR/zksolc/zksolc-remote-4cad2deaa6801d7a419f1ed6503c999948b0d6d8.0 https://github.com/matter-labs/era-compiler-solidity/releases/download/prerelease-a167aa3-code4rena/zksolc-linux-amd64-musl-v1.5.0


RUN wget -nv -O $COMPILER_DIR/zkvyper/compilerVersionInfo.json  "https://raw.githubusercontent.com/matter-labs/zkvyper-bin/main/version.json"

RUN (for ver in v1.3.13; do wget -nv -O $COMPILER_DIR/zkvyper/zkvyper-$ver  https://raw.githubusercontent.com/matter-labs/zkvyper-bin/main/linux-amd64/zkvyper-linux-amd64-musl-$ver; done)


# This matches VYPER_RELEASES_MIRROR_URL from hardhat-vyper
RUN wget -nv -O $COMPILER_DIR/vyper/linux/list.json https://vyper-releases-mirror.hardhat.org/list.json

# Currently we only use 0.3.10 release of vyper compiler (path taken from the list.json above)
RUN wget -nv -O $COMPILER_DIR/vyper/linux/0.3.10 https://github.com/vyperlang/vyper/releases/download/v0.3.10/vyper.0.3.10%2Bcommit.91361694.linux


# This matches COMPILER_REPOSITORY_URL from hardhat-core.
RUN wget -nv -O $COMPILER_DIR/linux-amd64/list.json https://binaries.soliditylang.org/linux-amd64/list.json

RUN (for ver in solc-linux-amd64-v0.8.20+commit.a1b79de6 solc-linux-amd64-v0.8.23+commit.f704f362 solc-linux-amd64-v0.8.24+commit.e11b9ed9; do \
    wget -nv -O $COMPILER_DIR/linux-amd64/$ver https://binaries.soliditylang.org/linux-amd64/$ver; \
    done)

RUN chmod -R +x /root/.cache/hardhat-nodejs/

RUN cd .. && rm -rf zksync-era
WORKDIR /usr/src/zksync
