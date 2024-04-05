#!/bin/bash

cd `dirname $0`

# Main zkSync contract interface
cat $ZKSYNC_HOME/contracts/l1-contracts/artifacts/cache/solpp-generated-contracts/bridgehub/IBridgehub.sol/IBridgehub.json | jq '{ abi: .abi}' > IBridgehub.json
cat $ZKSYNC_HOME/contracts/l1-contracts/artifacts/cache/solpp-generated-contracts/state-transition/IStateTransitionManager.sol/IStateTransitionManager.json | jq '{ abi: .abi}' > IStateTransitionManager.json
cat $ZKSYNC_HOME/contracts/l1-contracts/artifacts/cache/solpp-generated-contracts/state-transition/chain-interfaces/IZkSyncStateTransition.sol/IZkSyncStateTransition.json | jq '{ abi: .abi}' > IZkSyncStateTransition.json
# Default L1 bridge
cat $ZKSYNC_HOME/contracts/l1-contracts/artifacts/cache/solpp-generated-contracts/bridge/interfaces/IL1SharedBridge.sol/IL1SharedBridge.json | jq '{ abi: .abi}' > IL1SharedBridge.json
# Paymaster interface
cat $ZKSYNC_HOME/contracts/l2-contracts/artifacts-zk/cache-zk/solpp-generated-contracts/interfaces/IPaymasterFlow.sol/IPaymasterFlow.json | jq '{ abi: .abi}' > IPaymasterFlow.json
