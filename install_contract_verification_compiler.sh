#!/bin/bash

set -e

# install zksolc 1.3.x
skip_versions="v1.3.12 v1.3.15 v1.3.20" && \
    for VERSION in $(seq -f "v1.3.%g" 0 23); do \
    if echo " $skip_versions " | grep -q -w " $VERSION "; then \
    continue; \
    fi; \
    mkdir -p $(pwd)/etc/zksolc-bin/$VERSION && \
    wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-$VERSION -O $(pwd)/etc/zksolc-bin/$VERSION/zksolc && \
    chmod +x $(pwd)/etc/zksolc-bin/$VERSION/zksolc; \
    done

# install zkvyper 1.3.x
for VERSION in $(seq -f "v1.3.%g" 9 16); do \
    mkdir -p $(pwd)/etc/zkvyper-bin/$VERSION && \
    wget https://github.com/matter-labs/zkvyper-bin/raw/main/linux-amd64/zkvyper-linux-amd64-musl-$VERSION -O $(pwd)/etc/zkvyper-bin/$VERSION/zkvyper && \
    chmod +x $(pwd)/etc/zkvyper-bin/$VERSION/zkvyper; \
    done

# install solc
# Installs all solc versions from the github repo
wget -O list.txt https://github.com/ethereum/solc-bin/raw/gh-pages/linux-amd64/list.txt

# Iterate versions
for LN in $(cat list.txt)
do
    # Download
    wget https://github.com/ethereum/solc-bin/raw/gh-pages/linux-amd64/$LN

    # Get short version name
    temp="${LN#"solc-linux-amd64-v"}"
    version="${temp%\+*}"

    # Move and rename
    mkdir -p $(pwd)/etc/solc-bin/$version/
    mv $LN $(pwd)/etc/solc-bin/$version/solc
    chmod +x $(pwd)/etc/solc-bin/$version/solc

    ls $(pwd)/etc/solc-bin/
done

# Download zkVM solc
list=("0.8.23-1.0.0" "0.8.22-1.0.0" "0.8.21-1.0.0" "0.8.20-1.0.0" "0.8.19-1.0.0" "0.8.18-1.0.0" "0.8.17-1.0.0" "0.8.16-1.0.0" "0.8.15-1.0.0" "0.8.14-1.0.0" "0.8.13-1.0.0" "0.8.12-1.0.0" "0.8.11-1.0.0" "0.8.10-1.0.0" "0.8.9-1.0.0" "0.8.8-1.0.0" "0.8.7-1.0.0" "0.8.6-1.0.0" "0.8.5-1.0.0" "0.8.4-1.0.0" "0.8.3-1.0.0" "0.8.2-1.0.0" "0.8.1-1.0.0" "0.8.0-1.0.0" "0.7.6-1.0.0" "0.7.5-1.0.0" "0.7.4-1.0.0" "0.7.3-1.0.0" "0.7.2-1.0.0" "0.7.1-1.0.0" "0.7.0-1.0.0" "0.6.12-1.0.0" "0.6.11-1.0.0" "0.6.10-1.0.0" "0.6.9-1.0.0" "0.6.8-1.0.0" "0.6.7-1.0.0" "0.6.6-1.0.0" "0.6.5-1.0.0" "0.6.4-1.0.0" "0.6.3-1.0.0" "0.6.2-1.0.0" "0.6.1-1.0.0" "0.6.0-1.0.0" "0.5.17-1.0.0" "0.5.16-1.0.0" "0.5.15-1.0.0" "0.5.14-1.0.0" "0.5.13-1.0.0" "0.5.12-1.0.0" "0.5.11-1.0.0" "0.5.10-1.0.0" "0.5.9-1.0.0" "0.5.8-1.0.0" "0.5.7-1.0.0" "0.5.6-1.0.0" "0.5.5-1.0.0" "0.5.4-1.0.0" "0.5.3-1.0.0" "0.5.2-1.0.0" "0.5.1-1.0.0" "0.5.0-1.0.0" "0.4.26-1.0.0" "0.4.25-1.0.0" "0.4.24-1.0.0" "0.4.23-1.0.0" "0.4.22-1.0.0" "0.4.21-1.0.0" "0.4.20-1.0.0" "0.4.19-1.0.0" "0.4.18-1.0.0" "0.4.17-1.0.0" "0.4.16-1.0.0" "0.4.15-1.0.0" "0.4.14-1.0.0" "0.4.13-1.0.0" "0.4.12-1.0.0")
for version in ${list[@]};
do
    mkdir -p $(pwd)/etc/solc-bin/zkVM-$version/
    wget https://github.com/matter-labs/era-solidity/releases/download/$version/solc-linux-amd64-$version -O $(pwd)/etc/solc-bin/zkVM-$version/solc
    chmod +x $(pwd)/etc/solc-bin/zkVM-$version/solc
done

# install vyper
mkdir -p $(pwd)/etc/vyper-bin/0.3.3 \
    && wget -O vyper0.3.3 https://github.com/vyperlang/vyper/releases/download/v0.3.3/vyper.0.3.3%2Bcommit.48e326f0.linux \
    && mv vyper0.3.3 $(pwd)/etc/vyper-bin/0.3.3/vyper \
    && chmod +x $(pwd)/etc/vyper-bin/0.3.3/vyper
mkdir -p $(pwd)/etc/vyper-bin/0.3.9 \
    && wget -O vyper0.3.9 https://github.com/vyperlang/vyper/releases/download/v0.3.9/vyper.0.3.9%2Bcommit.66b96705.linux \
    && mv vyper0.3.9 $(pwd)/etc/vyper-bin/0.3.9/vyper \
    && chmod +x $(pwd)/etc/vyper-bin/0.3.9/vyper
mkdir -p $(pwd)/etc/vyper-bin/0.3.10 \
    && wget -O vyper0.3.10 https://github.com/vyperlang/vyper/releases/download/v0.3.10/vyper.0.3.10%2Bcommit.91361694.linux \
    && mv vyper0.3.10 $(pwd)/etc/vyper-bin/0.3.10/vyper \
    && chmod +x $(pwd)/etc/vyper-bin/0.3.10/vyper