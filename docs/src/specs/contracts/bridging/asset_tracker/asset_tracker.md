# Contract-based and Full AssetTracker

## Introduction

While interop allows for more possibilities for cross chain interaction, it also comes with additional risks: a malicious chain has more ways to impact other chains, since now direct communication between chains is available that does go through L1.

The `AssetTracker` is the main component that is used to ensure that even if a malicious chain is present, it will not affect the broader ecosystem. For now it is implemented in Solidity (and that's why interop is only allowed on top of a ZK Gateway).

## Glossary

- A "completely compromised" chain assumes that not only their proof system, but their facets can change arbitrarily. This is a relevant assumption since zksync os chains CTM will be controlled by a temporary multisig before final transition to the governance. 
- A "ZK compromised" means that the chain uses a canonical implementation with our Diamond Proxy, facets, etc, but its ZK proving system is compromised.

## Security assumptions

- All settlement layers are whitelisted and completely trusted: they are controlled by the decentralized governance and sufficient monitoring and defense in depth mechanisms ensure that their proof system will not be exploited.

> In the future, we want to allow untrusted settlement layers, however the scope of this release includes only trusted settlement layers. Any places that disallow it for now should be clearly marked in the codebase.

### Before v30

- Only era-based chains were settling on Gateway, and those are trusted fully (TODO: do we trust fully their proofs or just implementation? We should only assue that they can be ZK compromised).
- ZKsync OS based chains are possible. Before their upgrade system is migrated to the decentralized governance, they can be assumed to be potentially completely malicious even before the upgrade, but they can not settle on top of ZK based Gateway. Before v30 they only settle on L1.
- Only one whitelisted settlement layer exists and it is ZK Gateway. 

### After v30

- A new ZKsync OS powered settlement layer may be added. The transfer of control to decentralized governance is a prerequisite before the creation of such a settlement layer, i.e. then this settlement layer will also be completely trusted, but also zksync os chains will be able to become "ZK compromised" at most. Note, that the transfer of the ownership to governance is a prerequisite for ZKsync OS based settlement layer, not v30 upgrade in general.
- Chains can only migrate between L1 and ZK Gateway that belongs to their CTM (i.e. Era chains wont be able to migrate to ZKsync OS settlement layer and vice versa).

> Future compatibility note 1. In the future we want to allow Era-based chains to migrate on top of ZKsync OS powered Gateway. The codebase should be ready for it in general. Any places that disallow it for now should be clearly marked in the codebase.

> Future compatibility note 2. In the future we would want to support chains from untrusted, potentially completely malicious CTMs settling on top of ZKsync OS powered Gateywa. Any places that disallow it for now should be clearly marked in the codebase. 

## Before v30 (only L1<>L2 messaging)

Before v30, the chain balance was tracked only inside `L1NativeTokenVault`.

- the balances of the tokens for each chain are stored only on L1.
- when there is a deposit to the chain we increase it's relevant balance, opposite for withdrawals, we decrease the chain's balance for the token that's being withdrawn.
- For all tokens, we store the origin chainId. We don’t track the balance for that token on said chain, since the token can be minted arbitrarily.

```solidity
mapping(chainId => assetId => balance) chainBalance;
mapping(bytes32 assetId => uint256 originChainId) public originChainId;
```

## Contract based AssetTracker

To enable interop, while keeping the old 2FA mechanism, we would need a contract that would all outcoming messages from a chain (both interop messages and withdrawals) while ensuring that the chain does not spend more funds then it has. This contract is the asset tracker.

There are three types of asset trackers:

- `L1AssetTracker`. It took over the job of tracking chain balances on L1, that was previosuly held by `L1NativeTokenVault`. Note that due to costs, it does not support interop.
- `GWAssetTracker`. Its job is to ensure that chains that settle on top of a ZK Gateway do not send more funds in outgoing messages, then they have.
- `L2AssetTracker`. It is predeployed on all L2 chains. Its main purpose is to facilitate migration of chain between L1 and ZK Gateway as well as ensuring the secure behavior of tokens, native to this chain. More on it here (TODO).

### Invariants around `chainBalance`

The topic of migration between settlement layer will be discussed in the later sections. In this section, we'll discuss what the invariants around `chainBalance` look like and how they changed compared to the previous version.

#### Motivation

For comparison, in the past, `chainBalance[chainId][assetId]` had the following properties:

- Every time a chain `chainId` deposits `X` of `assetId` to the L1NativeTokenVault (only possible via L1->L2 communication), the `chainBalance[chainId][assetId]` gets increased by `X`.
- Every time a chain `chainId` withdraws `X` of `assetId` to the L1NativeTokenVault (either through withdrawals or claiming failed deposits), the `chainBalance[chainId][assetId]` got decreased by `X`.
- If a chain is the origin chain of the token, its `chainBalance` was not tracked.

Note, that the `chainBalance` for a chain was tracked *lazily*, i.e. the `chainBalance` displayed the difference between deposited and withdrawn funds from the shared bridge, not how much funds the chain actually has internally. This is an important distinction, e.g. a potentially malicious chain (obviously assuming totally compromised ZK) could *create*, but *not finalize* "withdraw" messages for more funds than the chain actually has. 

So in case of a compromised ZK, the `L1NativeTokenVault` and its `chainBalance` tracking protected the shared bridge, but it gave no guarantee about the validity of all the messages sent from the chain. The above is very neat for chains that only communicate with L1 as it is very cheap and provides easy isolation between the chains.

However, it becomes a big issue for interop: what if a compromised malicious chain that only holds 100 USC sends two messages, where each is worth 80 USDC? Only one of those messages could be processed. Putting the responsibility of accounting for each individual message on the recipient is hard and error prone, so it was decided that if a chains are to use interop, *each and every message* has be validated whenever it is added to the shared message tree.

Also, note that the previous way of using `chainBalance` stored on L1 for withdrawas is not an option, since a chain might have never deposited funds to the shared bridge, but only received those from the other chains via interop. Its `chainBalance` on L1 would be zero, but it does have the funds. In such cases, ZK Gateway, who approved the withdrawl via `GWAssetTracker` should be help responsible (and so its `chainBalance` should be reduced).

#### Invariants on L1

Similar to before, on L1, `chainBalance` will lazily store the amount of funds that the chain can withdraw from the shared bridge.

We say that a chain is responsible, i.e. its `chainBalance` should be reduced for a withdrawal/failed deposit, if:

1. The withdrawal/deposit happened before a chain upgraded to v30. Before v30 all chains were responsible for their own withdrawals/failed deposits.
2. The withdrawal happened when the chain settled on L1.
3. If it is a whitelisted settlement layer, it is responsible for all withdrawals for chains that settled there since they upgraded to v30.

Overall, the algorithm for determining whether to reduce the chain balance should look the following way:

```
1. If withdrawal is directly from L2 to L1 (chain didnt settle on GW), the chainBalance of the chain should be reduced.
2. Alternatively, if the chain did settle on GW, check whether the batch is before the chain has upgraded to v30.
    2a. If the batch is before the chain upgraded to v30, the chain's balance is reduced.
    2b. If the batch is after the chain upgraded to v30, the settlement layer's balance is reduced.
```

Note, that the above scheme relies on a fact that chains never lie about the time they upgraded to v30. This is ensured inside MessageRoot (TODO add link):
 - We only allow migrations between the same CTMs. This is because we the GW needs to trust the implementation of the zk chain contracts to provide the correct v30 upgrade batch number.
 - So we assume that chains that have non-zero v30 upgrade batch number, belong to either era CTM or ZKsync OS based CTM. For all the other CTMs, when a chain will be spawned, its "v30 upgrade batch number" will be started from 0.

Also, this relies on the fact that message verification needs to always happen against the root of GW that was submitted on L1, basically to ensure that the "Gateway" approved this messages and checked via `GWAssetTracker` that it was correct.

The above ensures that settlement layer has the ability to process interop internally, but also it puts heavy responsibility on it to ensure that each chain only does withdrawals/interops that do not exceed their "true" balance. 

Similarly to how it was done before, when a deposit of `assetId` to `chainId` is done, we attribute the balance to the `chainBalance` of either the chain itself (if it settles on L1) or its settlement layer (if it settles on top of it). More on the process of deposits when chain settles on top of a settlement layer, will be expanded later (here TODO).

> Note, that to avoid any sort of edge cases related to chain migration, we assume that each deposits is always fully processed on the settlement layer, i.e. a chain can only have a message for success or failure of a deposit when settling on top of Gateway only if this deposit was initiated settled on Gateway too and as a result when throguh GWAssetTracker (more on it later). Note, that we can not assume the contrary: a malicious chain MAY publish an arbitrary message when settling failed depoist/successful deposit on L1, since L1AssetTracker does not check contents of the message.

When chain migrates on top of GW, it needs to "give" some of its balance for internal usage of Gateway. More on the migration process will be described later (TODO), but basically a chain can tell that a certain amount of funds inside the `chainBalance` is still "active", i.e. maintained within the chain and so can be used for future interop/withdrawals etc.

If a chain is malicious it can provide an incorrect value of "active" balance, but it will only affect this chain:
- If chain keeps too much `chainBalance` for itself instead of giving it to its new settlement layer, it will miss out on interop/potenitally wont be able to settle correctly if it spends too much.
- If chain gives too much `chainBalance` for Gateway, then unfinalized L1 withdrawals for which the chain is responsible wont be able to get finalized.

When a chain migrates back to L1, GW can provide the amount of funds that the chain had at the time of the migration and so all these funds will be moved to the L1 `chainBalance` of the chain. In this case, the chain trusts the settlement layer to provide the correct value.

More on the processes above will be explained in the migration section (TODO).

#### Total sum invariant

Some tokens are native to chains (i.e. their original ERC20 contract is deployed there), so chains can not have any particular restrictions with regard 

Imagine the following scenario (`chain A` is malicious, while the rest of the chains are not):
- `chain A` mints 2^256-1 tokens and sends it to `chain B`.
- `chain A` mints 2^256-1 tokens and sends it to `chain C`.
- Now `chain B` tries to send 1 unit of token of `chain C` via interop. This leads to overflow of chain balance of `chain C`, making settlement of `chain B` impossible.

To prevent the issue from above, we want to ensure that overflows never happen. To do this we introduce the following new invariant:

*For both `L1AssetTracker` and `GWAssetTracker` for each `assetId` the sum of `chainBalance[chainId][assetId]` over all `chainId` is less than or equal to `type(uint256).max`.*

This invariant is held via the following restrictions:
- We track `chainBalance` for every chain, including the origin chain for the token, so it is possible that even an origin chain runs out of tokens.

On L1:
- All `chainBalance` start with 0. When a token is withdrawn/deposited for the first time (i.e. registered), the `chainBalance` for the responsible chain (either the origin chain or its current settlement layer) it set to `type(uint256).max`.
- Note, that `chainBalance` is tracked even for tokens whose origin is L1, i.e. their `chainBalance[l1_chain_id][assetId]` starts from the maximal value when it is deposited for the first time.

On `GWAssetTracker` for each `assetId` the sum of `chainBalance[chainId][assetId]` over all `chainId` is less than or equal to `L1AssetTracker.chainBalance[gw_chain_id][assetId]`. This is enforced by the following:
- All `chainBalance` start with 0. 
- When chains do interops, they do not change the sum of `chainBalance[chainId][assetId]`.
- The only way the sum of `chainBalance[chainId][assetId]` for a certain asset can be increased is when a deposit happens or a chain migrates it balance (TODO: link), i.e. the chain needs to "give" the same portion of its L1 chain balance to ZK Gateway.

`L2AssetTracker`'s job is to ensure that a malicious user or token can not cause the chain to fail to settle. For example, if a bad token allows sending multiple interops of `2^256-1` units of the same token, then the chain would violate the invariant (it would be caught inside `GWAssetTracker`) and so would fail to settle. On `L2AssetTracker`:
- `chainBalance[chainId][assetId]` is tracked only for balances of native assets for the current chain (TODO: is it really the case?). These start from `2^256-1` and then get reduced with each outbound transaction or increased with inbound transaction. This way, even if a token is malicious, it can not withdraw more than `2^256-1`, ensuring that chain always settles.

#### Migration of pre-v30 past chain balances

The process of the v30 upgrade is described here (TODO: link).









As mentioned before, before v30 we stored chain balances inside `L1NativeTokenVault`. All the old balances need to be transferred to their respective chains. The full process of the v30 upgrade will be described in a different section (TODO), here we mainly focus on how the `chainBalance` mapping is moved from the old contract to the new one.



####

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

The `AssetTracker` contract will be deployed on the GW and the L1, but interop and `processLogsAndMessages` is only enabled on GW, since using them on L1 is expensive. On L1 balances will be checked when the depsosit and withdrawals are processed. Thus, interop will not be available for chains that settle directly on L1.

For now the `AssetTracker` will only support tokens in the native token vault. These are assumed to have a single origin chain id, i.e. single chain where the supply of the token can change (via native minting or burning).

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

What happens if a token is not migrated to or from Gateway?

As the first security assumptin, on L1 `receiveMigrationOnL1` function, we check that the assetMigration number and the chainMigration number coincide, so legacy token migrations are not possible.

Secondly, the L1 chainBalance and Gw chainBalance added together always match the totalSupply of the token plus the outstanding withdrawals on L1 (i.e. withdrawals that not have been finalized yet). An invalid migration could only move the token balance between L1 and Gateway incorrectly.

This is not possible however. For GW -> L1 token balance migrations, we always migrate all the funds, reducing the Gateway chainBalance to zero, independently of how many migrations we missed. For L1->GW migrations the distribution could become incorrect if we missed the preceding GW-> L1 migration, since here we don't directly know the sum of the outstanding withdrawals on L1. If we miss the GW->L1 migration, we could transfer the totalSupply to GW mulitple times, without transferring back to L1. In order to avoid this, we always check that we have migrated back to L1. In this case the Gateway chainBalance is 0, so migration is safe.


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