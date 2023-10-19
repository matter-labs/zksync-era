#!/usr/bin/env bash

if [[ -z "${ZKSYNC_HOME}" ]]; then
  echo "Environment variable ZKSYNC_HOME is not set. Make sure it's set and pointing to the root of this repository"
  exit 1
fi

sed -i '' 's/^ETH_SENDER_SENDER_PROOF_SENDING_MODE.*$/ETH_SENDER_SENDER_PROOF_SENDING_MODE=OnlyRealProofs/' ../etc/env/dev.env
sed -i '' 's/^ETH_SENDER_SENDER_PROOF_LOADING_MODE.*$/ETH_SENDER_SENDER_PROOF_LOADING_MODE=FriProofFromGcs/' ../etc/env/dev.env
sed -i '' 's/^FRI_PROVER_SETUP_DATA_PATH.*$/FRI_PROVER_SETUP_DATA_PATH=vk_setup_data_generator_server_fri\/data\//' ../etc/env/dev.env
sed -i '' 's/^FRI_PROOF_COMPRESSOR_UNIVERSAL_SETUP_PATH.*$/FRI_PROOF_COMPRESSOR_UNIVERSAL_SETUP_PATH=..\/keys\/setup\/setup_2^26.key/' ../etc/env/dev.env
sed -i '' 's/^FRI_PROOF_COMPRESSOR_PROMETHEUS_PUSH_INTERVAL_MS.*$/FRI_PROOF_COMPRESSOR_PROMETHEUS_PUSH_INTERVAL_MS=100000000/' ../etc/env/dev.env

for i in {1..13}
do
    zk f cargo run --release --bin zksync_setup_data_generator_fri -- --numeric-circuit $i --is_base_layer
done

for i in {1..15}
do
    zk f cargo run --release --bin zksync_setup_data_generator_fri -- --numeric-circuit $i
done
