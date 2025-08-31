#!/bin/bash
set -euo pipefail

# === Get chain name (from input or default to "era") ===
CHAIN_NAME="${1:-era}"
export CHAIN_NAME

echo "STARTING BRIDGE ETH TO $CHAIN_NAME"

# === Load addresses from config ===
CONFIG="chains/$CHAIN_NAME/configs/contracts.yaml"

# === Load RPC URL from config ===
export RPC_URL=$(yq '.api.web3_json_rpc.http_url' chains/$CHAIN_NAME/configs/general.yaml)
export CHAIN_ID=$(cast chain-id -r "$RPC_URL")
echo "CHAIN_ID: $CHAIN_ID"

export BH_ADDRESS=$(yq '.ecosystem_contracts.bridgehub_proxy_addr' "$CONFIG")

# === Set constants ===
SENDER=0x36615Cf349d7F6344891B1e7CA7C72883F5dc049
PRIVATE_KEY=0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110
VALUE=10000000000000000000000000000000
GAS_LIMIT=10000000
GAS_PRICE=500000000
L1_RPC_URL=http://localhost:8545

# === Send transaction ===
cast send \
  --from "$SENDER" \
  --private-key "$PRIVATE_KEY" \
  "$BH_ADDRESS" \
  "requestL2TransactionDirect((uint256,uint256,address,uint256,bytes,uint256,uint256,bytes[],address))" \
  "($CHAIN_ID,$VALUE,$SENDER,0,0x00,1000000,800,[$PRIVATE_KEY],$SENDER)" \
  --gas-limit "$GAS_LIMIT" \
  --value "$VALUE" \
  --gas-price "$GAS_PRICE" \
  --rpc-url "$L1_RPC_URL"
