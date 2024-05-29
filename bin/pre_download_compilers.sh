#!/usr/bin/env bash

set -e

# This ./cache/hardhat-nodejs is coming from the env-paths module
# that hardhat is using.
COMPILER_DIR=/root/.cache/hardhat-nodejs/compilers-v2
mkdir -p $COMPILER_DIR/{/linux-amd64,/vyper/linux,/zksolc,/zkvyper}

# Fetch latest compiler version
wget -nv -O $COMPILER_DIR/zksolc/compilerVersionInfo.json  "https://raw.githubusercontent.com/matter-labs/zksolc-bin/main/version.json"


# These are the versions that we currently have in hardhat.config.ts in zksync-era and era-contracts.
# For now, if there is a new version of compiler, we'd have to modify this file.
# In the future, we should make it more automatic.
(for ver in v1.3.18 v1.3.21 v1.4.0 v1.4.1; do wget -nv -O $COMPILER_DIR/zksolc/zksolc-$ver  https://raw.githubusercontent.com/matter-labs/zksolc-bin/main/linux-amd64/zksolc-linux-amd64-musl-$ver; done)

# Special pre-release 1.5.0 compiler.
# It can be removed once system-contracts/hardhatconfig.ts stops using it.
wget -nv -O $COMPILER_DIR/zksolc/zksolc-remote-4cad2deaa6801d7a419f1ed6503c999948b0d6d8.0 https://github.com/matter-labs/era-compiler-solidity/releases/download/prerelease-a167aa3-code4rena/zksolc-linux-amd64-musl-v1.5.0


wget -nv -O $COMPILER_DIR/zkvyper/compilerVersionInfo.json  "https://raw.githubusercontent.com/matter-labs/zkvyper-bin/main/version.json"

(for ver in v1.3.13; do wget -nv -O $COMPILER_DIR/zkvyper/zkvyper-$ver  https://raw.githubusercontent.com/matter-labs/zkvyper-bin/main/linux-amd64/zkvyper-linux-amd64-musl-$ver; done)


# This matches VYPER_RELEASES_MIRROR_URL from hardhat-vyper
wget -nv -O $COMPILER_DIR/vyper/linux/list.json https://vyper-releases-mirror.hardhat.org/list.json

# Currently we only use 0.3.10 release of vyper compiler (path taken from the list.json above)
wget -nv -O $COMPILER_DIR/vyper/linux/0.3.10 https://github.com/vyperlang/vyper/releases/download/v0.3.10/vyper.0.3.10%2Bcommit.91361694.linux


# This matches COMPILER_REPOSITORY_URL from hardhat-core.
wget -nv -O $COMPILER_DIR/linux-amd64/list.json https://binaries.soliditylang.org/linux-amd64/list.json

(for ver in solc-linux-amd64-v0.8.20+commit.a1b79de6 solc-linux-amd64-v0.8.23+commit.f704f362 solc-linux-amd64-v0.8.24+commit.e11b9ed9; do \
    wget -nv -O $COMPILER_DIR/linux-amd64/$ver https://binaries.soliditylang.org/linux-amd64/$ver; \
    done)

chmod -R +x /root/.cache/hardhat-nodejs/
