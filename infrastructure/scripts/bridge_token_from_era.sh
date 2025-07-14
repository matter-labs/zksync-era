#!/bin/bash
set -euo pipefail

CONFIG_CONTRACTS="chains/era/configs/contracts.yaml"


# === Set contract addresses ===
export NTV_ADDRESS="0x0000000000000000000000000000000000010004"
export BH_ADDRESS="0x0000000000000000000000000000000000010002"
export L1_BH_ADDRESS=$(yq '.ecosystem_contracts.bridgehub_proxy_addr' "$CONFIG_CONTRACTS")
export ASSET_ROUTER_ADDRESS="0x0000000000000000000000000000000000010003"
export PRIVATE_KEY=0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110
export SENDER=0x36615Cf349d7F6344891B1e7CA7C72883F5dc049

# === Load RPC URL from config ===
export RPC_URL=$(yq '.api.web3_json_rpc.http_url' chains/era/configs/general.yaml)
echo "RPC URL: $RPC_URL"

# === Move into the contracts directory ===
cd contracts/l1-contracts/

# === Deploy test token ===
export TOKEN_ADDRESS=$(forge create ./contracts/dev-contracts/TestnetERC20Token.sol:TestnetERC20Token \
  --private-key $PRIVATE_KEY \
  --broadcast \
  --gas-price 10000 \
  --zksync \
  -r "$RPC_URL" \
  --constructor-args L2TestToken TT 18 | grep "Deployed to:" | awk '{print $3}'
)

# === Calculate token asset ID ===
CHAIN_ID=$(cast chain-id -r "$RPC_URL")
CHAIN_ID_HEX=$(printf "0x%02x\n" "$CHAIN_ID")

export TOKEN_ASSET_ID=$(cast keccak $(cast abi-encode "selectorNotUsed(uint256,address,address)" \
  "$CHAIN_ID_HEX" \
  "$NTV_ADDRESS" \
  "$TOKEN_ADDRESS"))

# === Encode token burn data ===
export TOKEN_BURN_DATA=$(cast abi-encode "selectorNotUsed(uint256,address,address)" \
  100 \
  $SENDER \
  "$TOKEN_ADDRESS" | cut -c 3-)

echo "Token Address: $TOKEN_ADDRESS"
echo "Token Asset ID: $TOKEN_ASSET_ID"

# === Mint tokens ===
cast send \
  --from $SENDER \
  --private-key $PRIVATE_KEY \
  "$TOKEN_ADDRESS" \
  "mint(address,uint256)" \
  $SENDER \
  1000000 \
  --rpc-url "$RPC_URL" \
  --gas-price 1000000000

# === Approve vault for transfer ===
cast send \
  --from $SENDER \
  --private-key $PRIVATE_KEY \
  "$TOKEN_ADDRESS" \
  "approve(address,uint256)" \
  "$NTV_ADDRESS" \
  100000000 \
  --rpc-url "$RPC_URL" \
  --gas-price 1000000000

# === Initiate withdrawal ===
withdrawTxHash=$(
  cast send \
    --from $SENDER \
    --private-key $PRIVATE_KEY \
    "$ASSET_ROUTER_ADDRESS" \
    "withdraw(bytes32,bytes)" \
    "$TOKEN_ASSET_ID" \
    "$TOKEN_BURN_DATA" \
    --gas-limit 10000000 \
    --rpc-url "$RPC_URL" \
    --gas-price 1000000000 \
    --json \
    | jq -r '.transactionHash'
#   | grep -i "transactionHash" | awk '{print $2}'
)
echo "Withdraw transaction hash: $withdrawTxHash"

forge script deploy-scripts/provider/ZKSProvider.s.sol:ZKSProvider --broadcast --slow --legacy --skip-simulation  --ffi --rpc-url http://localhost:8545 \
  --private-key $PRIVATE_KEY --sig "waitForWithdrawalToBeFinalized(uint256,address,string,bytes32,uint256)" \
  $CHAIN_ID  $L1_BH_ADDRESS  $RPC_URL  $withdrawTxHash 0

forge script deploy-scripts/provider/ZKSProvider.s.sol:ZKSProvider --broadcast --slow --legacy --skip-simulation  --ffi --rpc-url http://localhost:8545 \
  --private-key $PRIVATE_KEY --sig "finalizeWithdrawal(uint256,address,string,bytes32,uint256)" \
  $CHAIN_ID  $L1_BH_ADDRESS  $RPC_URL  $withdrawTxHash 0
