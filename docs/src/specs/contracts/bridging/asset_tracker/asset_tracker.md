# Contract-based and Full AssetTracker, ZK IP, Firewall

## Introduction

The AssetTracker is a component that provides additional security, by tracking the balance of chains on the chains settlement layer (Gateway and L1). This is done by parsing all interop txs on the SL and updating the balance of the chains.

### Name ideas

Firewall, ZK IP, AssetTracker. TBD.

## Bulkheads

Today, we have bulkheads inside NTV. 

- currently we store the balance of the token only on L1
- when chain deposits, withdraws from L1, we increase decrease the balance.
- For L2 Native tokens, we track the origin chainId. We don’t track the balance of that chain, since the token can be minted arbitrarily. We store the origin

```solidity
mapping(chainId => assetId => balance) chainBalance;
mapping(bytes32 assetId => uint256 originChainId) public originChainId;
```

### Contract based ZK IP  (internet protocol) / AssetTracker

ZK IP should :

- enable interop
    - Bulkheads do not work with interop, since interop changes the balance of a chain without touching L1.

For now the AssetTracker will only support tokens in the native token vault. These are assumed to have a single origin chain id, i.e. single chain where the supply of the token can change.

To enable interop, besides updating based on L1<>L2 txs, we will have to [parse all interop txs](https://github.com/matter-labs/era-contracts/blob/b5fda9c4dd8171ffb53337711fe8da43b4266026/l1-contracts/contracts/bridge/asset-tracker/AssetTrackerBase.sol#L35). This means we have to parse all L2toL1 logs, if the sender is the interopCenter parse the message, and update the balance on the SL. 

We can have non balance changing operations.

When settling each batch, each chain would call the following function: 

```solidity
// Called only when the batch is executed

contract AssetTracker {
	function processLogsAndMessages(ProcessLogsInput calldata _processLogsInputs) external;

	struct ProcessLogsInput {
		L2Log[] logs;
		bytes[] messages;
		uint256 chainId;
		uint256 batchNumber;
		bytes32 chainBatchRoot;
		bytes32 messageRoot;
	}
}
```

### Migrating and settling on Gateway

When a chain migrates from L1 to GW or from GW to L1, the `chainBalance` mapping for each token does not get transferred automatically. They can be trustlessly moved by anyone from GW to L1 and vice versa. But our standard tooling will migrate all token balances for best UX.

The AssetTracker contracts will be deployed on the GW and the L1, but interop and `processLogsAndMessages` is only enabled on GW, since using them on L1 is expensive. On L1 balances will be checked when the depsosit and withdrawals are processed.

On GW we process all incoming and outgoing messages to chains. L1->L2 messages are processed as they are sent through the Gateway, and L2->L1 messages are processed together with L2->L2 messages in the processLogsAndMessages function.

When a chain is settling on Gateway it has calls the `processLogsAndMessages` when settling. This means that if the token is not yet migrated, and a withdrawal is processed, the settlement will fail, since the chain's balance will be zero. In order to prevent this, withdrawals and interop will be paused on the chain until the token's balance is migrated.

Withdrawals on the chain do not need to be paused when migrating to L1, since the chain can settle without processing logs. However if the token balance is not yet migrated, the withdrawal will fail on L1.

#### Migrating to Gateway

When migrating each tokens balance to Gateway from L1, we increase the chainBalance of the GW, and decrease the balance of the chain.There are outstanding withdrawals on L1 from the L2, so we cannot migrate the whole balance of the chain. We can only migrate the amount that is not withdrawn, i.e. the amount that the token contract on the chain has as its `totalSupply`.

In order for information to flow smoothly, migrating to the Gateway has the following steps:

- Chain's AssetTracker reads the totalSupply of the token and sends a message to the Gateway.
- The L1 AssetTracker will receive the message and update the chainBalance mapping of the Chain and the Gateway, and forwards the message to GW as well as the L2.
- The Gateway's AssetTracker will receive the message and update the chainBalance mapping.
- The L2 AssetTracker will receive the message and update the chainMigration number.

#### Migration from Gateway

On Gateway all withdrawals are processed in the `processLogsAndMessages` function. This means there are no outstanding withdrawals, the chainBalance mapping will match the totalSupply of the token on the chain. This means that the whole balance of the chain can be migrated to L1. The steps are accordingly:

- The migration is initiated on the Gateway, the balance is decreased to zero and sent to L1.
- The L1 AssetTracker will receive the message and increase the chainBalance of the Chain and decrease the balance of the Gateway.
- A message is sent to the Gateway to update the chainBalance mapping.
- A message is sent to the L2 to increase the migration number of the token.

#### Missed token balance migrations

What happens if a token is not migrated to Gateway, the chain migrates back to L1, and then back to Gateway? This is not an issue, since the source of truth for this direction is the totalSupply of the token on the chain.

It might happen that a legacy (i.e. for a previous migration) L2->L1 message or GW->L1 message is used for withdrawals. In order to avoid this, we include the migration number in each token migration.


## How full ZK IP could look like with the same user interface (+ migration) could look like

Processing and updating all the logs on L1 or GW does not scale. Full ZK IP would be a zk validium running in parallel to the main chain, storing the balance of the chain and processing the logs. Instead of passing in all logs on the SL, we would only pass in a zk proof.  

For each chain we would still have the mappings, ( without the chainId, since it applies for the current chain):

```solidity
mapping(assetId => balance) chainBalance;
```

The main difference is that we cannot update the destination chain’s balance when sending the messages in `parseLogsAndMessages` since now the chainBalance mappings for different chains are on different chains. This means we will have to import the receiving/incoming messages. We do this the same way we do interop: via the MessageRoot, merkle proofs, and L2Nullifier to not double mint. 

```solidity
/// similar to the one on the L2
contract L2MessageRootStorageForAssetTracking {
	mapping(uint256 chainId => mapping(uint256 batchNumber => bytes32 msgRoot)) public msgRoots;
}

contract L2NullifierForAssetTracking {
	mapping(txHash => bool isConsumed)
}

contract InteropHandlerForAssetTracking {
	function executeBundle(InteropBundle) {
		...
	}
}
```

With each state transition:

- Chain provides a the root of the global tree `MessageRoot` that it used to apply add operations from.
- Root hash of the zk ip chain, this includes:
    - The new state of the nullifier
    - the state of the imported messageRoots ( these should be preserved )
    - new state of the balances, isMinterRole
- The chains exported messages FullMessageRoot
    - in FullMessageRoot = keccak(localRoot, AggregatedRoot)
    

### Withdrawals to L1/other chains

Withdrawals to L1 are just ZK IP messages that mint funds for L1. This is the same as for other L2s.

When a user wants to withdraw funds, it needs to provide a proof for the corresponding ZK IP message. 

  

### Migration to full ZK IP

When we are ready to move to the full ZK IP, we could add a operation available to everyone called: “consume bulkhead”, it would append an `add` operation to the `MessageRoot`. The chain can then consume this operation into its local tree later on.