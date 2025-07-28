#!/bin/bash
set -euo pipefail

# === Load addresses from config ===
CONFIG="chains/era/configs/contracts.yaml"

export BH_ADDRESS=$(yq '.ecosystem_contracts.bridgehub_proxy_addr' "$CONFIG")

# === Set constants ===
SENDER=0x36615Cf349d7F6344891B1e7CA7C72883F5dc049
PRIVATE_KEY=0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110
VALUE=10000000000000000000000000000000
GAS_LIMIT=10000000
GAS_PRICE=500000000
RPC_URL=http://localhost:8545

# === Send transaction ===
cast send \
  --from "$SENDER" \
  --private-key "$PRIVATE_KEY" \
  "$BH_ADDRESS" \
  "requestL2TransactionDirect((uint256,uint256,address,uint256,bytes,uint256,uint256,bytes[],address))" \
  "(271,$VALUE,$SENDER,0,0x00,1000000,800,[$PRIVATE_KEY],$SENDER)" \
  --gas-limit "$GAS_LIMIT" \
  --value "$VALUE" \
  --gas-price "$GAS_PRICE" \
  --rpc-url "$RPC_URL"
