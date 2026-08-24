#!/bin/bash

set -euo pipefail

# Install the stable upstream solc inventory from a frozen official-repository snapshot and verify
# each binary against that snapshot's manifest. Updating the inventory therefore requires review.
readonly SOLC_BIN_COMMIT="94bb5de935db866f1207818cd0e0154e1390ff3f"
./run_retried wget -O list.json \
  "https://raw.githubusercontent.com/ethereum/solc-bin/$SOLC_BIN_COMMIT/linux-amd64/list.json"
while IFS=$'\t' read -r binary version checksum; do
    destination="etc/solc-bin/$version/solc"
    mkdir -p "etc/solc-bin/$version"
    ./run_retried wget \
      "https://raw.githubusercontent.com/ethereum/solc-bin/$SOLC_BIN_COMMIT/linux-amd64/$binary" \
      -O "$destination"
    echo "${checksum#0x}  $destination" | sha256sum -c -
    chmod +x "$destination"
done < <(jq -r '.builds[] | [.path, .version, .sha256] | @tsv' list.json)

# Download zkVM solc
list=(
  "0.8.25-1.0.0" "0.8.24-1.0.0" "0.8.23-1.0.0" "0.8.22-1.0.0" "0.8.21-1.0.0" "0.8.20-1.0.0" "0.8.19-1.0.0" "0.8.18-1.0.0" "0.8.17-1.0.0" "0.8.16-1.0.0" "0.8.15-1.0.0" "0.8.14-1.0.0" "0.8.13-1.0.0" "0.8.12-1.0.0" "0.8.11-1.0.0" "0.8.10-1.0.0" "0.8.9-1.0.0" "0.8.8-1.0.0" "0.8.7-1.0.0" "0.8.6-1.0.0" "0.8.5-1.0.0" "0.8.4-1.0.0" "0.8.3-1.0.0" "0.8.2-1.0.0" "0.8.1-1.0.0" "0.8.0-1.0.0" "0.7.6-1.0.0" "0.7.5-1.0.0" "0.7.4-1.0.0" "0.7.3-1.0.0" "0.7.2-1.0.0" "0.7.1-1.0.0" "0.7.0-1.0.0" "0.6.12-1.0.0" "0.6.11-1.0.0" "0.6.10-1.0.0" "0.6.9-1.0.0" "0.6.8-1.0.0" "0.6.7-1.0.0" "0.6.6-1.0.0" "0.6.5-1.0.0" "0.6.4-1.0.0" "0.6.3-1.0.0" "0.6.2-1.0.0" "0.6.1-1.0.0" "0.6.0-1.0.0" "0.5.17-1.0.0" "0.5.16-1.0.0" "0.5.15-1.0.0" "0.5.14-1.0.0" "0.5.13-1.0.0" "0.5.12-1.0.0" "0.5.11-1.0.0" "0.5.10-1.0.0" "0.5.9-1.0.0" "0.5.8-1.0.0" "0.5.7-1.0.0" "0.5.6-1.0.0" "0.5.5-1.0.0" "0.5.4-1.0.0" "0.5.3-1.0.0" "0.5.2-1.0.0" "0.5.1-1.0.0" "0.5.0-1.0.0" "0.4.26-1.0.0" "0.4.25-1.0.0" "0.4.24-1.0.0" "0.4.23-1.0.0" "0.4.22-1.0.0" "0.4.21-1.0.0" "0.4.20-1.0.0" "0.4.19-1.0.0" "0.4.18-1.0.0" "0.4.17-1.0.0" "0.4.16-1.0.0" "0.4.15-1.0.0" "0.4.14-1.0.0" "0.4.13-1.0.0" "0.4.12-1.0.0"
  "0.8.28-1.0.1" "0.8.27-1.0.1" "0.8.26-1.0.1" "0.8.25-1.0.1" "0.8.24-1.0.1" "0.8.23-1.0.1" "0.8.22-1.0.1" "0.8.21-1.0.1" "0.8.20-1.0.1" "0.8.19-1.0.1" "0.8.18-1.0.1" "0.8.17-1.0.1" "0.8.16-1.0.1" "0.8.15-1.0.1" "0.8.14-1.0.1" "0.8.13-1.0.1" "0.8.12-1.0.1" "0.8.11-1.0.1" "0.8.10-1.0.1" "0.8.9-1.0.1" "0.8.8-1.0.1" "0.8.7-1.0.1" "0.8.6-1.0.1" "0.8.5-1.0.1" "0.8.4-1.0.1" "0.8.3-1.0.1" "0.8.2-1.0.1" "0.8.1-1.0.1" "0.8.0-1.0.1" "0.7.6-1.0.1" "0.7.5-1.0.1" "0.7.4-1.0.1" "0.7.3-1.0.1" "0.7.2-1.0.1" "0.7.1-1.0.1" "0.7.0-1.0.1" "0.6.12-1.0.1" "0.6.11-1.0.1" "0.6.10-1.0.1" "0.6.9-1.0.1" "0.6.8-1.0.1" "0.6.7-1.0.1" "0.6.6-1.0.1" "0.6.5-1.0.1" "0.6.4-1.0.1" "0.6.3-1.0.1" "0.6.2-1.0.1" "0.6.1-1.0.1" "0.6.0-1.0.1" "0.5.17-1.0.1" "0.5.16-1.0.1" "0.5.15-1.0.1" "0.5.14-1.0.1" "0.5.13-1.0.1" "0.5.12-1.0.1" "0.5.11-1.0.1" "0.5.10-1.0.1" "0.5.9-1.0.1" "0.5.8-1.0.1" "0.5.7-1.0.1" "0.5.6-1.0.1" "0.5.5-1.0.1" "0.5.4-1.0.1" "0.5.3-1.0.1" "0.5.2-1.0.1" "0.5.1-1.0.1" "0.5.0-1.0.1" "0.4.26-1.0.1" "0.4.25-1.0.1" "0.4.24-1.0.1" "0.4.23-1.0.1" "0.4.22-1.0.1" "0.4.21-1.0.1" "0.4.20-1.0.1" "0.4.19-1.0.1" "0.4.18-1.0.1" "0.4.17-1.0.1" "0.4.16-1.0.1" "0.4.15-1.0.1" "0.4.14-1.0.1" "0.4.13-1.0.1" "0.4.12-1.0.1"
)
for version in ${list[@]};
do
    mkdir -p etc/solc-bin/zkVM-$version/
    ./run_retried wget https://github.com/matter-labs/era-solidity/releases/download/$version/solc-linux-amd64-$version -O etc/solc-bin/zkVM-$version/solc
    chmod +x etc/solc-bin/zkVM-$version/solc
done

# Recent ZKsync solc forks used by current zksolc releases. These checksums are pinned from the
# immutable GitHub release assets so image rebuilds cannot silently pick up changed binaries.
recent_list=(
  "0.8.29-1.0.1:b57887d5a0adcb9419f65bf65092664f514522fba69f3c5a36d2bad105e02d53"
  "0.8.30-1.0.1:4722097f68a6489a75c93ec3cd314314b8faaedb6a2574332ba2ab7f0c957f5c"
  "0.8.22-1.0.2:c72042d546d73dd470471a7a7ff9eeb3ff2113bd3a7eb0afcf8ab92b036b42c9"
  "0.8.23-1.0.2:68ac9751cb62fe3c4c3365acdd07b569201929a7e7a7cdd0f740719fdcf5b106"
  "0.8.24-1.0.2:473508dc7108adf75bdc84325d57b19edde3f2cdf28f564a3086bd00e4a36871"
  "0.8.25-1.0.2:72a1ff7b1fc34a81da4883180ccb0cc2e154e19fb46b811350c60aac456d08ea"
  "0.8.26-1.0.2:95bb1c0e7bd23e3433fca0671a027d332eb698b395e8689a80076e3d62667bf4"
  "0.8.27-1.0.2:95f539cd2e2d9d77d07003337f6161eccca618947b5bd843dea298316aa1dcc1"
  "0.8.28-1.0.2:2b611f666099ed075eda6c7053e4901d0a70c63658a973fba7659a524f5ffe4d"
  "0.8.29-1.0.2:6266090290a8b6171e36d4ca7a9c4e5010d7d860ff6b51f56c4c0cbf21396a32"
  "0.8.30-1.0.2:50159c23fede9c666801c22ce1509a5785a4f6eeba750728a524860be420f49c"
)
for version_and_sha in "${recent_list[@]}"; do
    version="${version_and_sha%%:*}"
    checksum="${version_and_sha#*:}"
    destination="etc/solc-bin/zkVM-$version/solc"
    mkdir -p "etc/solc-bin/zkVM-$version"
    ./run_retried wget \
      "https://github.com/matter-labs/era-solidity/releases/download/$version/solc-linux-amd64-$version" \
      -O "$destination"
    echo "$checksum  $destination" | sha256sum -c -
    chmod +x "$destination"
done
