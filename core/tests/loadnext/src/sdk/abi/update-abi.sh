#!/bin/bash

cd `dirname $0`

# Main ZKsync contract interface
cat $ZKSYNC_HOME/contracts/l1-contracts/artifacts/contracts/bridgehub/IBridgehub.sol/IBridgehub.json | jq '{ abi: .abi}' > IBridgehub.json
cat $ZKSYNC_HOME/contracts/l1-contracts/artifacts/contracts/state-transition/IStateTransitionManager.sol/IStateTransitionManager.json | jq '{ abi: .abi}' > IStateTransitionManager.json
cat $ZKSYNC_HOME/contracts/l1-contracts/artifacts/contracts/state-transition/chain-interfaces/IZkSyncHyperchain.sol/IZkSyncHyperchain.json | jq '{ abi: .abi}' > IZkSyncHyperchain.json
# Default L1 bridge
cat $ZKSYNC_HOME/contracts/l1-contracts/artifacts/contracts/bridge/interfaces/IL1AssetRouter.sol/IL1AssetRouter.json | jq '{ abi: .abi}' > IL1AssetRouter.json
cat $ZKSYNC_HOME/contracts/l1-contracts/artifacts/contracts/bridge/interfaces/IL1ERC20Bridge.sol/IL1ERC20Bridge.json | jq '{ abi: .abi}' > IL1ERC20Bridge.json
# Paymaster interface
cat $ZKSYNC_HOME/contracts/l2-contracts/artifacts-zk/contracts/interfaces/IPaymasterFlow.sol/IPaymasterFlow.json | jq '{ abi: .abi}' > IPaymasterFlow.json
