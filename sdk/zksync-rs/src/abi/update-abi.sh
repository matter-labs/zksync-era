#!/bin/bash

cd `dirname $0`

# Main zkSync contract interface
cat $ZKSYNC_HOME/contracts/l1-contracts/artifacts/cache/solpp-generated-contracts/zksync/interfaces/IZkSync.sol/IZkSync.json | jq '{ abi: .abi}' > ZkSync.json
# Default L1 bridge
cat $ZKSYNC_HOME/contracts/l1-contracts/artifacts/cache/solpp-generated-contracts/bridge/interfaces/IL1Bridge.sol/IL1Bridge.json | jq '{ abi: .abi}' > L1Bridge.json
# Paymaster interface
cat $ZKSYNC_HOME/contracts/l2-contracts/artifacts-zk/contracts-preprocessed/interfaces/IPaymasterFlow.sol/IPaymasterFlow.json | jq '{ abi: .abi}' > IPaymasterFlow.json
