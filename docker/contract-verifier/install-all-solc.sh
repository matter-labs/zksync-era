#!/bin/bash

set -e

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
    mkdir -p etc/solc-bin/$version/
    mv $LN etc/solc-bin/$version/solc
    chmod +x etc/solc-bin/$version/solc

    ls etc/solc-bin/
done

# Download zkVM solc
list=("0.8.23-1.0.0" "0.8.22-1.0.0" "0.8.21-1.0.0" "0.8.20-1.0.0" "0.8.19-1.0.0" "0.8.18-1.0.0" "0.8.17-1.0.0" "0.8.16-1.0.0" "0.8.15-1.0.0" "0.8.14-1.0.0" "0.8.13-1.0.0" "0.8.12-1.0.0" "0.8.11-1.0.0" "0.8.10-1.0.0" "0.8.9-1.0.0" "0.8.8-1.0.0" "0.8.7-1.0.0" "0.8.6-1.0.0" "0.8.5-1.0.0" "0.8.4-1.0.0" "0.8.3-1.0.0" "0.8.2-1.0.0" "0.8.1-1.0.0" "0.8.0-1.0.0" "0.7.6-1.0.0" "0.7.5-1.0.0" "0.7.4-1.0.0" "0.7.3-1.0.0" "0.7.2-1.0.0" "0.7.1-1.0.0" "0.7.0-1.0.0" "0.6.12-1.0.0" "0.6.11-1.0.0" "0.6.10-1.0.0" "0.6.9-1.0.0" "0.6.8-1.0.0" "0.6.7-1.0.0" "0.6.6-1.0.0" "0.6.5-1.0.0" "0.6.4-1.0.0" "0.6.3-1.0.0" "0.6.2-1.0.0" "0.6.1-1.0.0" "0.6.0-1.0.0" "0.5.17-1.0.0" "0.5.16-1.0.0" "0.5.15-1.0.0" "0.5.14-1.0.0" "0.5.13-1.0.0" "0.5.12-1.0.0" "0.5.11-1.0.0" "0.5.10-1.0.0" "0.5.9-1.0.0" "0.5.8-1.0.0" "0.5.7-1.0.0" "0.5.6-1.0.0" "0.5.5-1.0.0" "0.5.4-1.0.0" "0.5.3-1.0.0" "0.5.2-1.0.0" "0.5.1-1.0.0" "0.5.0-1.0.0" "0.4.26-1.0.0" "0.4.25-1.0.0" "0.4.24-1.0.0" "0.4.23-1.0.0" "0.4.22-1.0.0" "0.4.21-1.0.0" "0.4.20-1.0.0" "0.4.19-1.0.0" "0.4.18-1.0.0" "0.4.17-1.0.0" "0.4.16-1.0.0" "0.4.15-1.0.0" "0.4.14-1.0.0" "0.4.13-1.0.0" "0.4.12-1.0.0")
for version in ${list[@]};
do
    mkdir -p etc/solc-bin/zkVM-$version/
    wget https://github.com/matter-labs/era-solidity/releases/download/$version/solc-linux-amd64-$version -O etc/solc-bin/zkVM-$version/solc
    chmod +x etc/solc-bin/zkVM-$version/solc
done
