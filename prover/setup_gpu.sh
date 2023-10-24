#!/usr/bin/env bash
# This script sets up the necessary data needed by the GPU FRI prover to be used locally.

if [[ -z "${ZKSYNC_HOME}" ]]; then
  echo "Environment variable ZKSYNC_HOME is not set. Make sure it's set and pointing to the root of this repository"
  exit 1
fi

sed -i.backup 's/^proof_sending_mode=.*$/proof_sending_mode="OnlyRealProofs"/' ../etc/env/base/eth_sender.toml
sed -i.backup 's/^proof_loading_mode=.*$/proof_loading_mode="FriProofFromGcs"/' ../etc/env/base/eth_sender.toml
rm ../etc/env/base/eth_sender.toml.backup
sed -i.backup 's/^setup_data_path=.*$/setup_data_path="vk_setup_data_generator_server_fri\/data\/"/' ../etc/env/base/fri_prover.toml
rm ../etc/env/base/fri_prover.toml.backup
sed -i.backup 's/^universal_setup_path=.*$/universal_setup_path="..\/keys\/setup\/setup_2^26.key"/' ../etc/env/base/fri_proof_compressor.toml
rm ../etc/env/base/fri_proof_compressor.toml.backup

zk config compile dev

for i in {1..13}
do
    zk f cargo run --features "gpu" --release --bin zksync_setup_data_generator_fri -- --numeric-circuit $i --is_base_layer
done

for i in {1..15}
do
    zk f cargo run --features "gpu" --release --bin zksync_setup_data_generator_fri -- --numeric-circuit $i
done
