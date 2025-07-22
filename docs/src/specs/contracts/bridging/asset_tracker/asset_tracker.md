# Contract-based and Full AssetTracker

## Introduction

The `AssetTracker` is a component that provides additional security, by tracking the balance of chains on the chains settlement layer (only Gateway and L1 as of now). This is done by parsing all interop transactions on the SL and updating the balance of the chains.

## Chain balance tracking for L1<>L2 transactions

Today, we have chain balance tracking inside NTV.

- currently we store the balances of the tokens for each chain only on L1.
- when there is a deposit to the chain we increase it's relevant balance, opposite for withdrawals, we decrease the chain's balance for the token that's being withdrawn.
- For L2 Native tokens, we store the origin chainId. We don’t track the balance for that token on said chain, since the token can be minted arbitrarily.

```solidity
mapping(chainId => assetId => balance) chainBalance;
mapping(bytes32 assetId => uint256 originChainId) public originChainId;
```

### Contract based AssetTracker

Asset tracker should enable interop, as interop does not go through L1, so the simple chain balance tracking is not enough.

To enable interop, besides updating based on L1<>L2 transactions, we will have to [parse all interop transactions](https://github.com/matter-labs/era-contracts/blob/6a53169ec3be11b33e27c1f11902a78bc6c31de1/l1-contracts/contracts/bridge/asset-tracker/AssetTracker.sol#L231). This means we have to parse all `L2toL1` logs, if the sender is the `interopCenter` parse the message, and update the balance on the SL. 

On GW we process all incoming and outgoing messages to chains. L1->L2 messages are processed as they are sent through the Gateway, and L2->L1 messages are processed together with L2->L2 messages in the `processLogsAndMessages` function.

When executing each batch, each chain would call the following function from the Executor facet:

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

#### Restrictions

The `AssetTracker` contract will be deployed on the GW and the L1, but interop and `processLogsAndMessages` is only enabled on GW, since using them on L1 is expensive. On L1 balances will be checked when the depsosit and withdrawals are processed.

For now the `AssetTracker` will only support tokens in the native token vault. These are assumed to have a single origin chain id, i.e. single chain where the supply of the token can change.

### Gateway

The Gateway component as an intermediate settlement layer complicates the setup. L1->GW->L2 deposits, failed deposits, and L2->GW->L1 withdrawals have to be handled.

First of all for chains settling on Gateway, the balance of the chain in the L1 `AssetTracker` will be moved to the Gateway's `chainBalance`. This is a complicated migration process, see [below](#migrating-and-settling-on-gateway). Similarly, when a chain migrates back to L1, its balance will be separate from the Gateway's `chainBalance`. On Gateway the `chainBalance`s are tracked for each chain separately in the GW's `AssetTracker`.

#### Deposits

When a user deposits to a chain settling on GW, an L1->GW transaction is sent, which add the transaction to the chains Priority Queue in the Mailbox facet on Gateway. On L1 the Gateway's `chainBalance` is updated. We will include the balance change in the in the L1->GW [forwardTransactionOnGatewayWithBalanceChange](https://github.com/matter-labs/era-contracts/blob/ba7b99eee1111fb7b87df7d6cc371aa21ea864b9/l1-contracts/contracts/interop/InteropCenter.sol#L392) function call, so that the Gateway `AssetTracker` can be updated.

#### Failed deposits

These L1->GW->L2 deposits might fail. This is handled on the L1 and on GW.

- In this case the `chainBalance` on L1 is updated when the user proves the `failedDeposit`, and the balance is subtracted from the Gateway's `chainBalance`. To know when a failed deposit happened on the GW or on L1, we [save](https://github.com/matter-labs/era-contracts/blob/6a53169ec3be11b33e27c1f11902a78bc6c31de1/l1-contracts/contracts/interop/InteropCenter.sol#L392) the settlement layer in the `L1Nullifier`.
- On GW the chain's `chainBalance` is decreased, when the chain executes its batch, and calls the `processLogsAndMessages` function. Each failed L1->L2 transaction produces a L2->L1 message, which we can verify in `processLogsAndMessages`. To know the balance change for the specific L1->L2 transaction, we save the balance change in the `forwardTransactionOnGatewayWithBalanceChange` function call. If the chain migrates to L1 before this step then it migrates with the increased chainBalance, and the failedDeposit will be verified on the L1.

#### Withdrawals

When a user withdraws from a chain settling on GW, we process the log on Gateway in `processLogsAndMessages`. This means that the balance of the chain is decreased on GW.

Similarly when finalizing the withdrawal on L1, we decrease the Gateway's `chainBalance`. We determine whether a withdrawal is processed on L1 or on GW by checking the `L1Nullifier` on L1, where we [save](https://github.com/matter-labs/era-contracts/blob/6a53169ec3be11b33e27c1f11902a78bc6c31de1/l1-contracts/contracts/bridge/L1Nullifier.sol#L537) the settlement layer.

### Migrating and settling on Gateway

When a chain migrates from L1 to GW or from GW to L1, the `chainBalance` mapping for each token does not get transferred automatically. They can be trustlessly moved by anyone from GW to L1 and vice versa. But our standard tooling will migrate all token balances for best UX.

When a chain is settling on Gateway it calls the `processLogsAndMessages` on settlement. This means that if the token is not yet migrated, and a withdrawal could be processed, the settlement would fail, since the chain's balance would be zero. In order to prevent this, withdrawals and interop will be paused on the chain until the token's balance is migrated.

The chain operator is responsible for migration of all token balances. However as each token migration costs L1 gas, this can be a spam attack vector. The chain can choose to ignore tokens, in which case the users can migrate the balances themselves. 

Withdrawals on the chain do not need to be paused when migrating to L1, since the chain can settle without processing logs. However if the token balance is not yet migrated, the withdrawal will fail on L1. It's not bad, since it doesn't break chain's settlement, in this case the withdrawal should be retried as usual when the withdrawal fails.

#### Migration number

To make the chain balance migration seamless, we expose the current settlement layer in the `Bootloader`. It then saves the settlement layer in the `SystemContext.sol` contract, and the `migrationNumber` (i.e. how many times the chain has migrated) gets saved in the `ChainAssetHandler.sol` contract. We also store the migration numbers for chains on the settlement layers.

We will also save the `assetMigrationNumber` in the `AssetTracker` contract. This show the last migration number where the token's balance was migrated. By comparing the `assetMigrationNumber` with the `migrationNumber`, we can determine if the token's balance was migrated or not, and if it is bridgeable or not.

#### Migrating to Gateway

When migrating each tokens balance to Gateway from L1, we increase the `chainBalance` of the GW, and decrease the balance of the chain. There are outstanding withdrawals on L1 from the L2, so we cannot migrate the whole balance of the chain. We can only migrate the amount that is not withdrawn, i.e. the amount that the token contract on the chain has as its `totalSupply`. This might change as there can be incoming deposits. We save the `totalSupply` at the first such deposit.

In order for information to flow smoothly, migrating to the Gateway has the following steps:

- Chain's `AssetTracker` gets the `totalSupply` of the token (from the token or from storage) and sends a message to L1.
- The L1 `AssetTracker` will receive the message and update the `chainBalance` mapping of the Chain and the Gateway, and forwards the message to GW as well as the L2.
- The Gateway's `AssetTracker` will receive the message and update the `chainBalance` mapping.
- The L2 `AssetTracker` will receive the message and update the `assetMigrationNumber`, enabling withdrawals and interop.

#### Migration from Gateway

On Gateway all withdrawals are processed in the `processLogsAndMessages` function. This means there are no outstanding withdrawals, the `chainBalance` mapping will match the totalSupply of the token on the chain. This means that the whole balance of the chain can be migrated to L1. The steps are accordingly:

- The migration is initiated on the Gateway, the balance is decreased to zero and sent to L1.
- The L1 `AssetTracker` will receive the message and increase the `chainBalance` of the Chain and decrease the balance of the Gateway.
- A message is sent to the Gateway to update the `chainBalance` mapping.
- A message is sent to the L2 to increase the migration number of the token.

#### Security Assumptions

What happens if a token is not migrated to Gateway, the chain migrates back to L1, and then back to Gateway? This is not an issue, since the source of truth for this direction is the totalSupply of the token on the chain.

It might happen that a legacy (i.e. for a previous migration) L2->L1 message or GW->L1 message is used for withdrawals. In order to avoid this, we include the migration number in each token migration.


<!-- ## How full ZK IP could look like with the same user interface (+ migration) could look like

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

When we are ready to move to the full ZK IP, we could add a operation available to everyone called: “consume bulkhead”, it would append an `add` operation to the `MessageRoot`. The chain can then consume this operation into its local tree later on. -->