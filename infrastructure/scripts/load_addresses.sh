#!/bin/bash
set -euo pipefail

# SECOND_CHAIN_NAME="${1:-validium}"
export SECOND_CHAIN_NAME="vbase_token"

export ERA_CONFIG_CONTRACTS="chains/era/configs/contracts.yaml"
export ERA_CONFIG_GENERAL="chains/era/configs/general.yaml"

export GW_CONFIG_CONTRACTS="chains/gateway/configs/contracts.yaml"
export GW_CONFIG_GENERAL="chains/gateway/configs/general.yaml"
export GW_CONFIG_GATEWAY="chains/gateway/configs/gateway.yaml"

export SC_CONFIG_CONTRACTS="chains/$SECOND_CHAIN_NAME/configs/contracts.yaml"
export SC_CONFIG_GENERAL="chains/$SECOND_CHAIN_NAME/configs/general.yaml"

export ERA_RPC_URL=$(yq '.api.web3_json_rpc.http_url' $ERA_CONFIG_GENERAL)
export GW_RPC_URL=$(yq '.api.web3_json_rpc.http_url' $GW_CONFIG_GENERAL)
export SC_RPC_URL=$(yq '.api.web3_json_rpc.http_url' $SC_CONFIG_GENERAL)

export NTV_ADDRESS=$(yq '.ecosystem_contracts.native_token_vault_addr' "$ERA_CONFIG_CONTRACTS")
export BH_ADDRESS=$(yq '.ecosystem_contracts.bridgehub_proxy_addr' "$ERA_CONFIG_CONTRACTS")
export L1_AR_ADDRESS=$(yq '.bridges.shared.l1_address' "$ERA_CONFIG_CONTRACTS")
export L1_AT_ADDRESS=$(cast call $NTV_ADDRESS "l1AssetTracker()(address)")
export CHAIN_ASSET_HANDLER_ADDRESS=$(cast call $BH_ADDRESS "chainAssetHandler()(address)")

export ERA_DIAMOND_PROXY_ADDRESS=$(yq '.era_diamond_proxy' "$ERA_CONFIG_CONTRACTS")
export GW_DIAMOND_PROXY_ADDRESS=$(yq '.era_diamond_proxy' "$GW_CONFIG_CONTRACTS")
export SC_DIAMOND_PROXY_ADDRESS=$(yq '.era_diamond_proxy' "$SC_CONFIG_CONTRACTS")

export GATEWAY_CTM_ADDRESS=$(yq '.state_transition_proxy_addr' "$GW_CONFIG_GATEWAY")
