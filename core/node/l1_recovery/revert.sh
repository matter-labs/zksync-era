cd ../../..
cargo run --bin block_reverter --release -- \
  --config-path chains/era/configs/general.yaml \
  --contracts-config-path chains/era/configs/contracts.yaml \
  --secrets-path chains/era/configs/secrets.yaml \
  --wallets-path chains/era/configs/wallets.yaml \
  --genesis-path chains/era/configs/genesis.yaml \
  send-eth-transaction --l1-batch-number 83
