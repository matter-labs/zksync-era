#!/bin/bash
set -euo pipefail

# === Get chain name (from input or default to "era") ===
CHAIN_NAME="${1:-era}"
export CHAIN_NAME

echo "STARTING BRIDGE TOKEN TO $CHAIN_NAME"

# === Load addresses from config ===
CONFIG_CONTRACTS="chains/$CHAIN_NAME/configs/contracts.yaml"
CONFIG_GENERAL="chains/$CHAIN_NAME/configs/general.yaml"
GENESIS_CONFIG="chains/$CHAIN_NAME/configs/genesis.yaml"


export NTV_ADDRESS=$(yq '.ecosystem_contracts.native_token_vault_addr' "$CONFIG_CONTRACTS")
export BH_ADDRESS=$(yq '.ecosystem_contracts.bridgehub_proxy_addr' "$CONFIG_CONTRACTS")
export L1_AR_ADDRESS=$(yq '.bridges.shared.l1_address' "$CONFIG_CONTRACTS")
export RPC_URL=$(yq '.api.web3_json_rpc.http_url' "$CONFIG_GENERAL")
export L1_CHAIN_ID=$(cast chain-id)
export PRIVATE_KEY=0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110
export SENDER=0x36615Cf349d7F6344891B1e7CA7C72883F5dc049

# === Load RPC URL from config ===
export RPC_URL=$(yq '.api.web3_json_rpc.http_url' chains/$CHAIN_NAME/configs/general.yaml)
export CHAIN_ID=$(yq '.l2_chain_id' "$GENESIS_CONFIG")
echo "CHAIN_ID: $CHAIN_ID"

# === Deploy test token ===
export TOKEN_ADDRESS=$(
  forge create ./contracts/l1-contracts/contracts/dev-contracts/TestnetERC20Token.sol:TestnetERC20Token \
    --private-key $PRIVATE_KEY \
    --broadcast \
    --gas-price 10000 \
    --constructor-args "TestToken" "TT" 18 |
    grep "Deployed to:" | awk '{print $3}'
)
echo "TOKEN_ADDRESS: $TOKEN_ADDRESS"
# export TOKEN_ADDRESS=0x9Db47305e174395f6275Ea268bE99Ab06b9b03f0;

# === Calculate asset ID ===
export TOKEN_ASSET_ID=$(cast keccak $(cast abi-encode "selectorNotUsed(uint256,address,address)" \
  $(printf "0x%02x\n" "$L1_CHAIN_ID") \
  0x0000000000000000000000000000000000010004 \
  "$TOKEN_ADDRESS"))

# === Build bridge calldata ===
ENCODED_PAYLOAD=$(cast abi-encode "selectorNotUsed(uint256,address,address)" \
  100 \
  $SENDER \
  "$TOKEN_ADDRESS" | cut -c 3-)

export BRIDGE_DATA="0x01${TOKEN_ASSET_ID:2}00000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060$ENCODED_PAYLOAD"

# === Output addresses ===
echo "TOKEN_ADDRESS: $TOKEN_ADDRESS"
echo "TOKEN_ASSET_ID: $TOKEN_ASSET_ID"

# === Mint and approve token ===
cast send --from $SENDER \
  --private-key $PRIVATE_KEY \
  "$TOKEN_ADDRESS" \
  "mint(address,uint256)" $SENDER 100 \
  --gas-price 10000

cast send --from $SENDER \
  --private-key $PRIVATE_KEY \
  "$TOKEN_ADDRESS" \
  "approve(address,uint256)" "$NTV_ADDRESS" 100 \
  --gas-price 10000

# === Send message through bridge ===
cast send --from $SENDER \
  --private-key $PRIVATE_KEY \
  "$BH_ADDRESS" \
  "requestL2TransactionTwoBridges((uint256,uint256,uint256,uint256,uint256,address,address,uint256,bytes))" \
  "(271,10000000000000000000000000000000,0,10000000,800,$SENDER,$L1_AR_ADDRESS,0,$BRIDGE_DATA)" \
  --gas-limit 10000000 \
  --value 10000000000000000000000000000000 \
  --rpc-url localhost:8545 \
  --gas-price 100000
