#!/bin/bash
set -euo pipefail

export CONFIG_CONTRACTS="chains/era/configs/contracts.yaml"
export CONFIG_GENERAL="chains/era/configs/general.yaml"

export NTV_ADDRESS=$(yq '.ecosystem_contracts.native_token_vault_addr' "$CONFIG_CONTRACTS")
export BH_ADDRESS=$(yq '.ecosystem_contracts.bridgehub_proxy_addr' "$CONFIG_CONTRACTS")
export L1_AR_ADDRESS=$(yq '.bridges.shared.l1_address' "$CONFIG_CONTRACTS")
export L1_IC_ADDRESS=$(cast call $BH_ADDRESS "interopCenter()(address)")
export L1_AT_ADDRESS=$(cast call $L1_IC_ADDRESS "assetTracker()(address)")
export CHAIN_ASSET_HANDLER_ADDRESS=$(cast call $BH_ADDRESS "chainAssetHandler()(address)")