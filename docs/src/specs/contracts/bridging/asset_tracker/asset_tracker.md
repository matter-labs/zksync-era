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

### Restrictions

The `AssetTracker` contract will be deployed on the GW and the L1, but interop and `processLogsAndMessages` is only enabled on GW, since using them on L1 is expensive. On L1 balances will be checked when the depsosit and withdrawals are processed. Thus, interop will not be available for chains that settle directly on L1.

For now the `AssetTracker` will only support tokens in the native token vault. These are assumed to have a single origin chain id, i.e. single chain where the supply of the token can change (via native minting or burning).

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

TODO move: As mentioned before, before v30 we stored chain balances inside `L1NativeTokenVault`. All the old balances need to be transferred to their respective chains. The full process of the v30 upgrade will be described in a different section (TODO), here we mainly focus on how the `chainBalance` mapping is moved from the old contract to the new one.

### Gateway asset tracker

#### Settlement of chains and interop

As previously mentioned, for security reasons, we want to ensure that it is enforced by the system that every interop transaction that is added to the shared tree is backed up by the necessary amount of tokens. Also, note that ZK Gateway is responsible for any withdrawals that chains perform on top of it. 

Thus, to ensure that chains always consume only the balance they have and interop is safe, we will have to [parse every single message that the chain sends](https://github.com/matter-labs/era-contracts/blob/6a53169ec3be11b33e27c1f11902a78bc6c31de1/l1-contracts/contracts/bridge/asset-tracker/AssetTracker.sol#L231). This means we have to parse all `L2toL1` logs, and if the sender is the `interopCenter` parse the message, and update the `chainBalance` of both the sender and recipient on the settlement layer.

On GW we process all incoming and outgoing messages to chains. L1->L2 messages are processed as they are sent through the Gateway, and L2->L1 messages are processed together with L2->L2 messages in the `processLogsAndMessages` function.

When executing each batch, each chain would call the following function from the Executor facet:

```solidity
// Called only when the batch is executed

contract GWAssetTracker {
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

#### Deposits through Gateway

When a user deposits to a chain settling on GW, an L1->GW transaction is sent, which add the transaction to the chains Priority Queue in the Mailbox facet on Gateway. On L1 the Gateway's `chainBalance` is updated. We will include the balance change in the L1->GW [forwardTransactionOnGatewayWithBalanceChange](https://github.com/matter-labs/era-contracts/blob/ba7b99eee1111fb7b87df7d6cc371aa21ea864b9/l1-contracts/contracts/interop/InteropCenter.sol#L392) function call, so that the `GWAssetTracker` can update the balance of chain. 

You can read about how the deposit flow starts on L1 here (TODO: link).

#### Failed deposits

These L1->GW->L2 deposits might fail. This is handled on the L1 and on GW.

- On L1, when the user proves the `failedDeposit`, the balance is subtracted from the Gateway's `chainBalance`.
- On GW the chain's `chainBalance` is decreased, when the chain executes its batch, and calls the `processLogsAndMessages` function. Each failed L1->L2 transaction produces a L2->L1 message, which we can verify in `processLogsAndMessages`. To know the balance change for the specific L1->L2 transaction, we save the balance change in the `forwardTransactionOnGatewayWithBalanceChange` function call.

Note, that to ensure that the logic above is always secure, `GWAssetTracker` is also responsible for maintaining an invariant that all deposits that were submitted when the chain was settling on top of ZK Gateway must be processed *before* the chain tries to leave Gateway.

This way, if a deposit failed, the ZK Gateway knows that it should keep enough funds to serve all these potential failed withdrawals when a chain will try to move out from ZK Gateway. (TODO: here).

#### Withdrawals

When a user withdraws from a chain settling on GW, we process the log on Gateway in `processLogsAndMessages`. This means that the balance of the chain is decreased on GW. Similarly to failed deposits, it means that these funds will be kept (TODO) by the ZK Gateway to ensure that it can always serve unfinalized withdrawals.

Similarly when finalizing the withdrawal on L1, we decrease the Gateway's `chainBalance`. We determine whether a withdrawal is processed on L1 or on GW by checking the `L1Nullifier` on L1, where we [save](https://github.com/matter-labs/era-contracts/blob/6a53169ec3be11b33e27c1f11902a78bc6c31de1/l1-contracts/contracts/bridge/L1Nullifier.sol#L537) the settlement layer.

### Migrating and settling on Gateway

When a chain migrates from L1 to GW or from GW to L1, the `chainBalance` mapping for each token does not get transferred automatically. They can be trustlessly moved by anyone from GW to L1 and vice versa. But our standard tooling will migrate all token balances for best UX.

When a chain is settling on Gateway it calls the `processLogsAndMessages` on settlement. This means that if the token is not yet migrated, and a withdrawal could be processed, the settlement would fail, since the chain's balance would be zero. In order to prevent this, withdrawals and interop will be paused on the chain until the token's balance is migrated.

The chain operator is responsible for migration of all token balances. However as each token migration costs L1 gas, this can be a spam attack vector. The chain can choose to ignore tokens, in which case the users can migrate the balances themselves. 

Withdrawals on the chain do not need to be paused when migrating to L1, since the chain can settle without processing logs. However if the token balance is not yet migrated, the withdrawal will fail on L1. It's not bad, since it doesn't break chain's settlement, in this case the withdrawal should be retried as usual when the withdrawal fails.

#### Migration number

To make the chain balance migration seamless, we expose the current settlement layer in the `Bootloader`. It then saves the settlement layer in the `SystemContext.sol` contract, and the `migrationNumber` (i.e. how many times the chain has migrated) gets saved in the `L2ChainAssetHandler.sol` contract. We also store the migration numbers for chains on the settlement layers.

You can read more about `ChainAssetHandler` here (TODO).

We will also save the `assetMigrationNumber` in the `AssetTracker` contract. This show the last migration number where the token's balance was migrated. By comparing the `assetMigrationNumber` with the `migrationNumber`, we can determine if the token's balance was migrated or not, and if it is bridgeable or not.

#### Migrating to Gateway

When migrating a balance of an `assetId` to Gateway from L1, we increase the `L1AssetHandler.chainBalance[gw_chain_id][assetId]`, and decrease the balance of the chain. There are outstanding withdrawals on L1 from the L2, so we cannot migrate the whole balance of the chain. We can only migrate the amount that is not withdrawn, i.e. the amount that the token contract on the chain has as its `totalSupply`. This might change as there can be incoming deposits. We save the `totalSupply` at the first such deposit inside `L2AssetHandler.savedTotalSupply` variable.

In order for information to flow smoothly, migrating to the Gateway has the following steps:

- `L2AssetTracker.initiateL1ToGatewayMigrationOnL2` is called by any user. This function obtains the current `totalSupply` of the token. For bridged assets, it is just `totalSupply` of those, since their implementation is known to be `BridgedStandardERC20`. For native tokens, we use `chainBalance` as it stores how much the chain has left.
- The `L1AssetTracker` will receive the message and update the `chainBalance` mapping of the Chain and the Gateway, and forwards the message to GW as well as the L2.
- The Gateway's `AssetTracker` will receive the message and update the `chainBalance` mapping.
- The L2 `AssetTracker` will receive the message and update the `assetMigrationNumber`, enabling withdrawals and interop.

#### Migration from Gateway

On Gateway all withdrawals are processed in the `processLogsAndMessages` function. This means that the `chainBalance` mapping will match the totalSupply of the token on the chain. This means that the whole balance of the chain can be migrated to L1. The steps are accordingly:

- The migration is initiated on the Gateway via `GWAssetTracker.initiateGatewayToL1MigrationOnGateway` function, the balance is decreased to zero and sent to L1.
- The L1 `AssetTracker` will receive the message and increase the `chainBalance` of the Chain and decrease the balance of the Gateway.
- A message is sent to the Gateway to update the `chainBalance` mapping.
- A message is sent to the L2 to increase the migration number of the token.

#### Disabling deposits during migrations

Let's recall the deposit invariant from here (TODO: link): all deposits that go through GW must be fully processed inside of it: they should be initiated when chain settles there and should be processed inside batches while chain settles there.

It is forced inside `GWAssetTracker` and every deposit that the chain processed inside the batch must've went through GW first. So if a chain accidentally has a deposit unexecuted on L1 and needs to settle on GW, it wont be able to do so. It is the job of the chain's implementation (done inside `AdminFacet.forwardedBridgeBurn`) to ensure that.

`GWAssetTracker` also forces on its own that every deposit that went through GW has been processed there as well.

But the above means that if a chain is permissionless, users could DDoS it with deposits never allowing a chain to actually migrate. To provide a solution suitable for permissionless chains, we added the ability to temporarily disable all incoming priority transactions: `AdminFacet.pauseDepositsAndInitiateMigration`.

This solution works even for permissionless chains, since it requires a cool off period, i.e. the deposits are paused for `PAUSE_DEPOSITS_TIME_WINDOW_START` but a new freeze can not be enabled sooner than `PAUSE_DEPOSITS_TIME_WINDOW_END`. Currently, `PAUSE_DEPOSITS_TIME_WINDOW_START` is equal to 3.5 days, while `PAUSE_DEPOSITS_TIME_WINDOW_END` is equal to 7 days.

#### Replay protection and edge cases with messaging

Our L2->L1 (or GW->L1) messages are valid in perpetuity. Typically, for actions like token withdrawals, we used `L1Nullifier` contract to store the nullifier that ensures that the same message can not be replayed twice.

However, for migration-related messages for the ease of implementation we used the `migrationNumber` and `assetMigrationNumber`: An asset-migrating message can be only processed on L1 once, since to process it, the carried `migrationNumber` needs to be greater than the current `assetMigrationNumber`.

TODO: the current system allows for some of the old token withdrawals to never get finalized, this is bad, we'll have to rework it.

#### Recovering from missed migrations

What happens if did not migrate a tokens' balance?

- If a chain settles on L1, then only withdrawals wont be able to get finalized since `L1AssetTracker.chainBalance` of the chain would be too low.
- If a chain settles on GW, and it tries to make an outbound operation (e.g. withdrawal) then it wont be able to settle. It is the job of the chain's `L2AssetTracker` to ensure that no withdrawals can happen until the chain balance has been migrated to ZK Gateway.

The above are just standard scenarios during migration, but what happens if e.g. a chain migrates to L1, does not finalize migration for the asset and then migrates back to Gateway?

For GW->L1 token balance migrations, we always migrate all the funds, reducing the Gateway chainBalance to zero, independently of how many migrations we missed. 

For L1->GW token balance migration we need to ensure that all the balance have been moved from ZK Gateway. This is enforced `L1AssetTracker`. This is needed for simplicity since `L2AssetTracker` does not know which funds are still left on Gateway and which ones are on L1 already. So we always demand that `assetMigrationNumber` is even, i.e. the migration to L1 has been complete and the chain balance of GW is zero (except for tokens that it needs for pending withdrawals).

### Deposit flow with L1AssetTracker

This section dives deeper into how `L1AssetTracker` is used to perform deposits for chains that settle on top of ZK Gateway. As already said before, when a deposit happens to the chain that settles on top of ZK Gateway, the balance accrual is assigned to ZK Gateway. However, the ZK Gateway also needs to increase the balance of the chain itself inside Gateway.

So when a message is relayed through ZK Gateway, ZK Gateway needs to query somehow how much funds were obtained by Gateway during the deposit. We could pass this data along during the deposit, but it is very hard to do so in a trustless manner without major changes to the codebase, it is the Mailbox of the L2 ZK chain that asks for the message to be relayed and this ZK chain may be potentially malicious and provide wrong values to the chain.

Thus, it was decided that whenever a deposit happens, `L1AssetTracker` should provide an interface for the ZK Gateway to query the deposited data. The approach below is used:

1. When some token is accrued by a chain (`handleChainBalanceIncreaseOnL1` internal function), if a chain settles on top of ZK Gateway, we set in transient storage `_setTransientBalanceChange` the assetId and the amount deposited.
2. Later, the L1Bridgehub calls the Mailbox of the receiving L2 chain, which would then relay the message to the Gateway's mailbox by calling `requestL2TransactionToGatewayMailboxWithBalanceChange`.
3. Then, the ZK Gateway's Mailbox would call `L1AssetRouter.consumeBalanceChange` to "consume" this balance increase (i.e. reset it to 0, while reading the assetId and the amount). The ZK Gateway would then know the amount of funds and asset that should be attached with the message.

#### Security notes around the deposits

Firstly, it is important to note that in the current implementation, we trust the chain to provide the correct `baseTokenAmount` to relay, i.e. the transient store scheme from above is only used for consuming ERC20 deposits. This obviously means that only chains the L1 implementation of which ZK Gateway can trust (i.e. the same CTM) are allowed to settle on ZK Gateway.

Next, it is important to discuss the edge cases around the transient storage. To avoid any double spending, the only way to read the transient values is to irreversibly consume them once. So regardless of any actions of malicious actors, a single balance increase will only be relayed to GW only once. From this invariant we know that the sum of chainbalances inside GW will never exceed GW's balance on L1.

However, the above opens doors for another potential error: someone maliciosuly consuming the variable for the chain. To prevent this, only whitelisted settlement layers are allowed to consume the balance. A second potential issue is overwrites: someone could overwrite the variable, but reentering and trying to make the deposit twice.

TODO: explain how we combat overwrites, today it is a "require", but it does not work with l2 bridges deposits. 

### Withdrawal flow with L1AssetTracker

This section dives deeper into how `L1AssetTracker` is used to perform withdrawals for chains. Unlike deposits during which we definitely know the current settlement layer for the chain, a chain might've had a history of moving on and off from a settlement layer (including different ZK Gateways, L1, etc).

Withdrawals are always initiated from L1Nullifier (there may be some legacy methods that eventually end up calling L1Nullfier, so the principle is always the same). Similarly to deposits, where we needed to know the chain to increase the balance of (the chain itself or its settlement layer), we need to know which chain's balance to reduce. 

To get the chain id of the settlement layer for a particular withdrawal (or claimed deposit), we look at the proof for the message. You read more about its recursive format here (TODO: link to message root doc), but in a nutshell, a proof is a recursive structure that starts from the L2 chain's tree root and then (if chain settled on top of some Gateway), it ends with the GW's tree.

If the proof is of depth 1, i.e. it does not have any recursion, it means that the withdrawal belonged to the chain. If not, the withdrawal belonged to its settlement layer (the only exception is based on v30 upgrade, you can read more about in the sections above TODO).

After verifying the correctness of the message, L1Nullifier stores inside `TRANSIENT_SETTLEMENT_LAYER_SLOT` and `TRANSIENT_SETTLEMENT_LAYER_SLOT+1` the settlement layer at the time of withdrawal and the batch number when withdrawal happened (the batch number would be needed for the checks against the time when the chain upgraded to v30).

To ensure secure verification, the `L1MessageRoot` is used (TODO: link to doc).

Then, when processing the decrease of the balance, the `L1AssetTracker` would read the values from above and decide which chain to reduce the balance of:
- If the withdrawal is from before the chain upgraded to v30, the chain is responsible.
- Otherwise, the settlement layer is responsible.

#### Security notes around withdrawals

Unlike with deposits, we dont "consume" the transient stored values, i.e. it can always be read.

The invariant that is maintained is that right after the L1Nullifier is called, there can never be any untrusted calls until the `L1AssetTracker` is called (this ensures that nothing overwrites the previous values).

> Note, that it means that for now, only `NativeTokenVault` is supported in conjunction with the asset tracker as the asset handler must have trusted implementation.

Also, we say that the only way `handleChainBalanceIncreaseOnL1` can be called is that it must be preceeded by a valid L1Nullifier call.

#### Claiming failed deposits

With regards to L1AssetRouter, the process of claiming failed deposits is extremely similar to withdrawals, i.e. the same `handleChainBalanceIncreaseOnL1` is called and the same invariants around L1Nullifier transient storage are expected.

### L1 and L2 native token registration

As discussed previously in the section about sum invariants (TODO: link), to ensure that that sum of chainBalances for each asset is lower than 2^256-1, all chain balances must originate from L1 (starting from `2^256-1` for origin chain at the start the first time). We will refer to the process of initializing the token's chainBalances for the first as "registration". 

Note, that for this section we will only consider tokens that are interacted with for the first time starting from v30. To explore what the migration for pre-v30 assets looks like, check out the doc (TODO: upgrade link doc).

When a token is bridged for the first time, `registerNewToken` function is called, it would assign the max balance to the origin chain of the token. This function is only used in `L1AssetTracker` and `L2AssetTracker` (not `GWAssetTracker` TODO please check with Kalman).

#### L1AssetTracker

There are 3 ways how a token is registered on L1AssetRouter:
- For the first time through deposit. This is applicable for L1 native assets, these are only ones for which the first time of registration is deposit, during which `chainBalance[block.chainid][assetId]` is assigned to the maximal value (reduced by the deposited amount of course). 
- For the first time through withdrawals. This provides nice UX for L2-native assets that are withdrawn for the first time.
- `registerL2NativeToken`. We will talk about the use case for it later in this section.

The first two approaches work fine for chains that only settle on top of L1. However, chains may settle on top of a ZK Gateway. To use a token inside ZK Gateway, it needs its balance reflected inside ZK Gateway's chain balance. 

Usually, for a token that has been already registered on L1, we would've just used the standard procedure of balance migrations from L1 to Gateway (TODO: link). So in theory the following could be done for a token:
- Withdrawal of 0 tokens (to formally register it on L1).
- Chain balance migration to GW (to ensure that GW recognizes its balance).

However, the above flow means that the chain would have to wait 3 hours of the timelock (TODO: link) before settling on L1. Even if the timelock is removed, the waiting time would still be bound by proving and full finalization of GW batches on L1. To provide a much better UX, we introduced a new function: `registerL2NativeToken`:
- It will ensure that the token is registered for the first time and assign the balance on L1 directly to the ZK Gateway and send confirmation to both ZK Gateway and the chain itself. 

This way, the chain only needs to wait for the L1->GW and L1->GW->L2 tx to get processed, which makes the process a lot faster.

#### L2AssetTracker

The main job of the L2AssetTracker in this context is to ensure that if a chain settles on top of Gateway, it will not be able to withdraw until its balance has been either migrated (TODO: link) or it has been registered via the `registerL2NativeToken` explained above.

It is done by comparing `assetMigrationNumber` of the asset with the chain migration number and if those dont coincide, then not allowing to migrate.

TODO: currently one can register l2 token + finalize withdrawal at the same time, while breaking the invariant.

TODO: check that L2AssetTracker wont actually allow to withdraw such token.

TODO: maybe write down invairants for `assetMigrationNumber` for l2 asset tracker in comments in the code.

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