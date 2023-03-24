#!/bin/bash

cd `dirname $0`

OPEN_ZEPPELIN_CONTRACTS=$ZKSYNC_HOME/contracts/ethereum/artifacts/@openzeppelin/contracts
ETHEREUM_CONTRACTS=$ZKSYNC_HOME/contracts/ethereum/artifacts/cache/solpp-generated-contracts
ZKSYNC_CONTRACTS=$ZKSYNC_HOME/contracts/zksync/artifacts-zk/cache-zk/solpp-generated-contracts
SYSTEM_CONTRACTS=$ZKSYNC_HOME/etc/system-contracts/artifacts-zk/cache-zk/solpp-generated-contracts

cat $OPEN_ZEPPELIN_CONTRACTS/token/ERC20/extensions/IERC20Metadata.sol/IERC20Metadata.json | jq '{ abi: .abi}' > IERC20.json

cat $ETHEREUM_CONTRACTS/bridge/interfaces/IL1Bridge.sol/IL1Bridge.json | jq '{ abi: .abi}' > IL1Bridge.json
cat $ETHEREUM_CONTRACTS/zksync/interfaces/IZkSync.sol/IZkSync.json | jq '{ abi: .abi}' > IZkSync.json
cat $ETHEREUM_CONTRACTS/common/interfaces/IAllowList.sol/IAllowList.json | jq '{ abi: .abi}' > IAllowList.json

cat $ZKSYNC_CONTRACTS/bridge/interfaces/IL2Bridge.sol/IL2Bridge.json | jq '{ abi: .abi}' > IL2Bridge.json
cat $ZKSYNC_CONTRACTS/interfaces/IPaymasterFlow.sol/IPaymasterFlow.json | jq '{ abi: .abi}' > IPaymasterFlow.json

cat $SYSTEM_CONTRACTS/interfaces/IL1Messenger.sol/IL1Messenger.json | jq '{ abi: .abi}' > IL1Messenger.json
cat $SYSTEM_CONTRACTS/interfaces/IEthToken.sol/IEthToken.json | jq '{ abi: .abi}' > IEthToken.json
cat $SYSTEM_CONTRACTS/ContractDeployer.sol/ContractDeployer.json | jq '{ abi: .abi}' > ContractDeployer.json
