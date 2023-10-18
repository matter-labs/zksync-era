#!/usr/bin/env bash

for i in {1..13}
do
    zk f cargo run --release --bin zksync_setup_data_generator_fri -- --numeric-circuit $i --is_base_layer
done

for i in {1..15}
do
    zk f cargo run --release --bin zksync_setup_data_generator_fri -- --numeric-circuit $i
done

sed -i '' 's/^ETH_SENDER_SENDER_PROOF_SENDING_MODE.*$/ETH_SENDER_SENDER_PROOF_SENDING_MODE=OnlyRealProofs/' dev.env
sed -i '' 's/^ETH_SENDER_SENDER_PROOF_LOADING_MODE.*$/ETH_SENDER_SENDER_PROOF_LOADING_MODE=FriProofFromGcs/' dev.env
sed -i '' 's/^FRI_PROVER_SETUP_DATA_PATH.*$/FRI_PROVER_SETUP_DATA_PATH=vk_setup_data_generator_server_fri\/data\//' dev.env
