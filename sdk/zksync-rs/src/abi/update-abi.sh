#!/bin/bash

cd `dirname $0`

# Main zkSync contract interface
cat $ZKSYNC_HOME/contracts/ethereum/artifacts/cache/solpp-generated-contracts/bridgehead/interfaces/IBridgehead.sol/IBridgehead.json | jq '{ abi: .abi}' > IBridgehead.json
cat $ZKSYNC_HOME/contracts/ethereum/artifacts/cache/solpp-generated-contracts/proof-system/proof-system-interfaces/IProofSystem.sol/IProofSystem.json | jq '{ abi: .abi}' > IProofSystem.json
# cat $ZKSYNC_HOME/contracts/ethereum/artifacts/cache/solpp-generated-contracts/era/interfaces/Era.sol/IEra.json | jq '{ abi: .abi}' > IEra.json
# Default L1 bridge
cat $ZKSYNC_HOME/contracts/ethereum/artifacts/cache/solpp-generated-contracts/bridge/interfaces/IL1Bridge.sol/IL1Bridge.json | jq '{ abi: .abi}' > IL1Bridge.json
# Paymaster interface
cat $ZKSYNC_HOME/contracts/zksync/artifacts-zk/cache-zk/solpp-generated-contracts/interfaces/IPaymasterFlow.sol/IPaymasterFlow.json | jq '{ abi: .abi}' > IPaymasterFlow.json
