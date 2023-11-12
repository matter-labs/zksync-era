#!/usr/bin/env bash
# This script sets up the necessary data needed by the CPU FRI prover to be used locally.

GPU_FLAG=""
if [ "$1" = "gpu" ]; then
    GPU_FLAG='--features "gpu"'
fi

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
    if ! [ -f vk_setup_data_generator_server_fri/data/setup_basic_${i}_data.bin ]; then
        zk f cargo run $GPU_FLAG --release --bin zksync_setup_data_generator_fri -- --numeric-circuit $i --is_base_layer
    fi
done

if ! [ -f vk_setup_data_generator_server_fri/data/setup_scheduler_data.bin ]; then
    zk f cargo run $GPU_FLAG --release --bin zksync_setup_data_generator_fri -- --numeric-circuit 1
fi

if ! [ -f vk_setup_data_generator_server_fri/data/setup_node_data.bin ]; then
    zk f cargo run $GPU_FLAG --release --bin zksync_setup_data_generator_fri -- --numeric-circuit 2
fi

for i in {3..15}
do
    if ! [ -f vk_setup_data_generator_server_fri/data/setup_leaf_${i}_data.bin ]; then
        zk f cargo run $GPU_FLAG --release --bin zksync_setup_data_generator_fri -- --numeric-circuit $i
    fi
done
