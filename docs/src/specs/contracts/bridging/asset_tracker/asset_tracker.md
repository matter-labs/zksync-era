# Contract-based and Full AssetTracker

## Introduction

While interop allows for more possibilities for cross chain interaction, it also comes with additional risks: a malicious chain has more ways to impact other chains, since now direct communication between chains is available that does not go through L1.

The `AssetTracker` is the main component that is used to ensure that even if a malicious chain is present, it will not affect the broader ecosystem. For now it is implemented in Solidity (and that's why interop is only allowed on top of a ZK Gateway).

## Glossary

- A "completely compromised" chain assumes that not only their proof system, but their facets can change arbitrarily. This is a relevant assumption since ZKsync OS chains CTM will be controlled by a temporary multisig before final transition to the governance. 
- A "ZK compromised" means that the chain uses a canonical implementation with our Diamond Proxy, facets, etc, but its ZK proving system is compromised, due to an unexpected bug in ZK circuits.

## Security assumptions

- All settlement layers are whitelisted and are trusted to be "ZK compromised" at most, i.e. their implementation is controlled by the decentralized governance, however the system should be robust in cases the ZK proof system of a settlement layer is compromised. More about it can be read [here](../../gateway/trust_assumptions.md).
- What's more, until Stage 1 is reached, chains that migrate on top of Gateway should assume that Gateway's operator is reasonably trusted, e.g. he will not censor transactions unnecessarily for prolonged periods of time. Note that, in this release, Stage 1 is available only for chains that settle on L1. More on it [here](../../chain_management/stage1.md).

> In the future, we want to allow completely untrusted settlement layers, however the scope of this release includes only trusted settlement layers. Any places that disallow it for now should be clearly marked with comments in the codebase.

### Before v31

- Only era-based chains were settling on a ZK Gateway, and those are trusted to be "ZK Compromised" at worst, since their implementation is controlled by the decentralized governance.
- ZKsync OS based chains are possible. Before their upgrade system's ownership is migrated to the decentralized governance, they can be assumed to be potentially completely malicious even before the upgrade, but they can not settle on top of ZK based Gateway. Before v31 they only settle on L1.
- Only one whitelisted settlement layer exists and it is EraVM based ZK Gateway. 

### At the moment of the upgrade

At the moment of the upgrade we will demand that all chains have migrated to L1, i.e. the number of chains that settle on top of ZK Gateway is 0. Also, the old Era-based ZK Gateway will be shut down and will have its whitelisted settlement layer status revoked.

By "shut down" we mean completely abandoned and it will not participate in the v31 upgrade.

### After v31

- A new ZKsync OS powered settlement layer may be added. The transfer of control to decentralized governance is a prerequisite before the creation of such a settlement layer, i.e. then this settlement layer will also be completely trusted, but also ZKsync OS chains will be able to become "ZK compromised" at most. Note, that the transfer of the ownership to governance is a prerequisite for ZKsync OS based settlement layer, *not* v31 upgrade in general.
- Chains should be able to migrate to the ZKsync OS powered ZK Gateway, once it is added. However, it is assumed that chains from only trusted CTMs can settle on top of our ZK Gateway.

> Future compatibility note. In the future we would want to support chains from untrusted, potentially completely malicious CTMs settling on top of ZKsync OS powered Gateway. Any places that disallow it for now should be clearly marked in the codebase. 

## How chain balances were tracked before v31 (only L1<>L2 messaging)

Before v31, the chain balance was tracked only inside `L1NativeTokenVault`.

- The balances of the tokens for each chain are stored only on L1.
- When there is a deposit to the chain we increase it's relevant balance, opposite for withdrawals, we decrease the chain's balance for the token that's being withdrawn.
- For all tokens, we store the origin chainId. We don’t track the balance for that token on said chain, since the token can be minted arbitrarily.

```solidity
mapping(chainId => assetId => balance) chainBalance;
mapping(bytes32 assetId => uint256 originChainId) public originChainId;
```

## Contract based AssetTracker

To enable interop, while keeping the old 2FA mechanism, we would need a contract that would read all outcoming messages from a chain (both interop messages and withdrawals) while ensuring that the chain does not spend more funds than it has. This contract is the asset tracker.

There are three types of asset trackers:

- `L1AssetTracker`. It took over the job of tracking chain balances on L1, that was previously held by `L1NativeTokenVault`. Note that due to costs, it does not support interop.
- `GWAssetTracker`. Its job is to ensure that chains that settle on top of a ZK Gateway do not send more funds in outgoing messages than they have.
- `L2AssetTracker`. It is predeployed on all L2 chains. Its main purpose is to facilitate migration of chain between L1 and ZK Gateway as well as ensuring the secure behavior of tokens, native to their respective chains. More on it [here](#l2assettracker).

### Restrictions

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

However, it becomes a big issue for interop: what if a compromised malicious chain that only holds 100 USC sends two messages, where each is worth 80 USDC? Only one of those messages could be processed. Putting the responsibility of accounting for each individual message on the recipient is hard and error prone, so it was decided that if chains are to use interop, *each and every message* has to be validated whenever it is added to the shared message tree.

Also, note that the previous way of using `chainBalance` stored on L1 for withdrawals is not an option, since a chain might have never deposited funds to the shared bridge, but only received those from the other chains via interop. Its `chainBalance` on L1 would be zero, but it does have the funds. In such cases, ZK Gateway, who approved the withdrawal via `GWAssetTracker` should be held responsible (and so its `chainBalance` should be reduced).

#### Invariants on L1

Similar to before, on L1, `chainBalance` will lazily store the amount of funds that the chain can withdraw from the shared bridge.

We say that a chain is responsible, i.e. its `chainBalance` should be reduced for a withdrawal/failed deposit, if:

1. The withdrawal/deposit happened before a chain upgraded to v31. Before v31 all chains were responsible for their own withdrawals/failed deposits.
2. The withdrawal happened when the chain settled on L1.
3. If it is a whitelisted settlement layer, it is responsible for all withdrawals for chains that settled there since they upgraded to v31.

Overall, the algorithm for determining whether to reduce the chain balance should look the following way:

```
1. If withdrawal is directly from L2 to L1 (chain didnt settle on GW), the chainBalance of the chain should be reduced.
2. Alternatively, if the chain did settle on GW, check whether the batch is before the chain has upgraded to v31.
    2a. If the batch is before the chain upgraded to v31, the chain's balance is reduced.
    2b. If the batch is after the chain upgraded to v31, the settlement layer's balance is reduced.
```

Note, that the above scheme relies on a fact that chains never lie about the time they upgraded to v31. This is ensured inside MessageRoot (see [v31UpgradeChainBatchNumber](../../interop/message_root.md#v31upgradechainbatchnumber)):
 - Additionally we also assume that chains that have non-zero v31 upgrade batch number while settling on top of a ZK Gateway, belong to a CTM controlled by the decentralized governance (either Era CTM or ZKsync OS CTM after the ownership migration). For all the other CTMs, when a chain will be spawned, its "v31 upgrade batch number" will be started from 0.
 - Note, that during the v31 upgrade, the chains the implementation of which is not controlled by the decentralized governance may theoretically lie about their v31 upgrade batch number, so before enabling any settlement layer, the governance will have to double check that no chain did so.   

Also, this relies on the fact that message verification needs to always happen against the root of GW that was submitted on L1, basically to ensure that the "Gateway" approved this messages and checked via `GWAssetTracker` that it was correct.

The above ensures that settlement layer has the ability to process interop internally, but also it puts heavy responsibility on it to ensure that each chain only does withdrawals/interops that do not exceed their "true" balance.

Similarly to how it was done before, when a deposit of `assetId` to `chainId` is done, we attribute the balance to the `chainBalance` of either the chain itself (if it settles on L1) or its settlement layer (if it settles on top of it). More on the process of deposits when a chain settles on top of a settlement layer is explained [here](#deposits-through-gateway).

When chain migrates on top of GW, it needs to "give" some of its balance for internal usage of Gateway. More on the migration process will be described [here](#migrating-and-settling-on-gateway), but basically a chain can tell that a certain amount of funds inside the `chainBalance` is still "active", i.e. maintained within the chain and so can be used for future interop/withdrawals etc.

If a chain is malicious it can provide an incorrect value of "active" balance, but it will only affect this chain:
- If chain keeps too much `chainBalance` for itself instead of giving it to its new settlement layer, it will miss out on interop/potentially won't be able to settle correctly if it spends too much.
- If chain gives too much `chainBalance` for Gateway, then unfinalized L1 withdrawals for which the chain is responsible wont be able to get finalized.

When a chain migrates back to L1, GW can provide the amount of funds that the chain had at the time of the migration and so all these funds will be moved to the L1 `chainBalance` of the chain. In this case, the chain trusts the settlement layer to provide the correct value.

More on the processes above will be explained in the migration section [here](#migrating-and-settling-on-gateway).

#### Total sum invariant

Some tokens are native to chains (i.e. their original ERC20 contract is deployed there), so in theory infinite amount of tokens could be spawned from such an origin chain. However, allowing to spawn arbitrary amounts comes with issues.

Imagine the following scenario (`chain A` is malicious, while the rest of the chains are not):
- `chain A` mints 2^256-1 tokens and sends it to `chain B`.
- `chain A` mints 2^256-1 tokens and sends it to `chain C`.
- Now `chain B` tries to send 1 unit of token of `chain C` via interop. This leads to overflow of chain balance of `chain C`, making settlement of `chain B` impossible.

To prevent the issue from above, we want to ensure that overflows never happen. To do this we introduce the following new invariant:

*For both `L1AssetTracker` and `GWAssetTracker` for each `assetId` the sum of `chainBalance[chainId][assetId]` over all `chainId` is less than or equal to `type(uint256).max`.*

This invariant is held by tracking `chainBalance` for every chain, including the origin chain for the token, so it is possible that even an origin chain runs out of tokens.

On `L1AssetTracker`:
- All `chainBalance` start with 0. When a token is withdrawn/deposited for the first time (i.e. registered), the `chainBalance` for the origin chain is set to `type(uint256).max`.
- Note, that `chainBalance` is tracked even for tokens whose origin is L1, i.e. their `chainBalance[l1_chain_id][assetId]` starts from the maximal value when it is deposited for the first time.

On `GWAssetTracker` for each `assetId` the sum of `chainBalance[chainId][assetId]` over all `chainId` is less than or equal to `L1AssetTracker.chainBalance[gw_chain_id][assetId]`. This is enforced by the following:
- All `chainBalance` start with 0. 
- When chains do interops, they do not change the sum of `chainBalance[chainId][assetId]` (when a chain claims interop, it receives the same funds as the ones that were removed when the interop was sent).
- The only way the sum of `chainBalance[chainId][assetId]` for a certain asset can be increased is when a deposit happens or a chain migrates it balance [here](#migrating-to-gateway), i.e. the chain needs to "give" the same portion of its L1 chain balance to ZK Gateway.

`L2AssetTracker`'s job is to ensure that a malicious user or token can not cause the chain to fail to settle. For example, if a bad token allows sending multiple interops of `2^256-1` units of the same token, then the chain would violate the invariant (it would be caught inside `GWAssetTracker`) and so would fail to settle. On `L2AssetTracker`:
- `chainBalance[chainId][assetId]` is tracked only for balances of native assets for the current chain. These start from `2^256-1` and then get reduced with each outbound transaction or increased with inbound transaction. This way, even if a token is malicious, it can not withdraw more than `2^256-1`, ensuring that chain always settles.

#### Migration of pre-v31 past chain balances

The invariants above are very easy to hold for post-v31 tokens. However those that were present before v31, the process is a bit harder. The process of the v31 upgrade is described [here](../../upgrade_history/v31-bundles/upgrade_process_v31.md).

### Gateway asset tracker

#### Settlement of chains and interop

As previously mentioned, for security reasons, we want to ensure that it is enforced by the system that every interop transaction that is added to the shared tree is backed up by the necessary amount of tokens. Also, note that ZK Gateway is responsible for any withdrawals that chains perform on top of it.

Thus, to ensure that chains always consume only the balance they have and interop is safe, we will have to parse every single message that the chain sends. This means we have to parse all `L2toL1` logs, and if the sender is the `interopCenter` parse the message, and update the `chainBalance` of both the sender and recipient on the settlement layer.

Note, that when a chain sends interop, the sender chains' balance is decreased immediately and the recipient's one is increased immediately as well. This allows a chain to claim this withdrawal internally even after it migrates away from ZK Gateway.

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

When a user deposits to a chain settling on GW, an L1->GW transaction is sent, which add the transaction to the chains Priority Queue in the Mailbox facet on Gateway. On L1 the Gateway's `chainBalance` is updated. We will include the balance change in the L1->GW `forwardTransactionOnGatewayWithBalanceChange` function call, so that the `GWAssetTracker` can update the balance of chain. 

You can read about how the deposit flow starts on L1 [here](#deposit-flow-with-l1assettracker).

#### Failed deposits

These L1->GW->L2 deposits might fail. This is handled on the L1 and on GW.

- On L1, when the user proves the `failedDeposit`, the balance is subtracted from the Gateway's `chainBalance`.
- On GW the chain's `chainBalance` is decreased, when the chain executes its batch, and calls the `processLogsAndMessages` function. Each failed L1->L2 transaction produces a L2->L1 message, which we can verify in `processLogsAndMessages`. To know the balance change for the specific L1->L2 transaction, we save the balance change in the `forwardTransactionOnGatewayWithBalanceChange` function call.

This way, if a deposit failed, the ZK Gateway knows that it should keep enough funds to serve all these potential failed withdrawals when a chain will try to move out from ZK Gateway.

Note, that Gateway can only process a deposit only when this deposit went through Gateway. It is the responsibility of the chain to ensure that when it migrates to Gateway, it has no outstanding priority transactions. It is current checked inside `AdminFacet.forwardedBridgeBurn`.

#### Withdrawals

When a user withdraws from a chain settling on GW, we process the log on Gateway in `processLogsAndMessages`. This means that the balance of the chain is decreased on GW. Similarly to failed deposits, it means that these funds will be kept by the ZK Gateway to ensure that it can always serve unfinalized withdrawals.

Similarly when finalizing the withdrawal on L1, we decrease the Gateway's `chainBalance`. We determine whether a withdrawal is processed on L1 or on GW by checking the `L1Nullifier` on L1, where we save the settlement layer. More on it [here](#withdrawal-flow-with-l1assettracker).

### Migrating and settling on Gateway

When a chain migrates from L1 to GW or from GW to L1, the `chainBalance` mapping for each token does not get transferred automatically. They can be trustlessly moved by anyone from GW to L1 and vice versa. But our standard tooling will migrate all token balances for best UX.

When a chain is settling on Gateway it calls the `processLogsAndMessages` on settlement. This means that if the token is not yet migrated, and a withdrawal is processed, the settlement would fail, since the chain's balance would be zero. In order to prevent this, withdrawals and interop will be paused on the chain (via `L2AssetTracker`) until the token's balance is migrated.

Anyone can migrate token balances permissionless, though typically it would be the chain operator that migrates them. This ensures that even if there are lots of tokens to migrate, the operator may choose to ignore rarely used assets that can be migrated by users at their own pace. 

Withdrawals on the chain do not need to be paused when migrating to L1, since the chain can settle without processing logs. However if the token balance is not yet migrated, the withdrawal finalization will fail on L1. It doesn't break chain's settlement, in this case the withdrawal can be retried after the balance have been migrated to L1.

#### Migration number

To make the chain balance migration seamless, we expose the current settlement layer in the `Bootloader`. It then saves the settlement layer in the `SystemContext.sol` contract, and the `migrationNumber` (i.e. how many times the chain has migrated) gets saved in the `L2ChainAssetHandler.sol` contract. We also store the migration numbers for chains on the settlement layers.

We will also save the `assetMigrationNumber` in the `AssetTracker` contract. This mapping stores the last migration number where the token's balance was migrated. By comparing the `assetMigrationNumber` with the `migrationNumber`, we can determine if the token's balance was migrated or not, and if it is bridgeable or not.

#### Migrating to Gateway

When migrating a balance of an `assetId` to Gateway from L1, we increase the `L1AssetHandler.chainBalance[gw_chain_id][assetId]`, and decrease the balance of the chain. There are outstanding withdrawals on L1 from the L2, so we cannot migrate the whole balance of the chain. We need to keep the amount that is needed to cover the outstanding withdrawals and failed deposits. I.e. we need to keep 

```
total_withdrawals_when_settle_on_l1 + total_failed_deposits_when_settle_on_l1 - total_claimed_from_l1_balance
```

Since it is very hard to track `total_failed_deposits_when_settle_on_l1`, we can rewrite the above as:

```
total_withdrawals_when_settle_on_l1 + total_deposits_when_settle_on_l1 - total_successful_deposits_when_settle_on_l1 - total_claimed_from_l1_balance
```

So when trying to migrate the chain balance from L1 to GW:

The chain will send the following two numbers to L1:

- `totalWithdrawnToL1`
- `totalSuccessfulDepositsFromL1`
    - Note that it is not possible to track `totalFailedDeposits` since failed deposit = execution revert = no state changes. We will use the difference between total and successful deposits to track this value.

While on L1 we need to track:

- `totalClaimedL1` (updated with each claimed withdrawal or failed deposit)
- `totalDepositsFromL1` (updated with each deposit)

On L1, we will calculate the following:

```
totalFailedDeposits = totalDepositsFromL1 - message.totalSuccessfulDepositsFromL1
l1BalanceToKeep = message.totalWithdrawalsToL1 + totalFailedDeposits - totalClaimedSoFar
```

Note, that to ensure that the value is fully representative:

- This must happen after the chain has changed its settlement layer + disabled withdrawals before the token is migrated.
    - Since the token is not migrated ⇒ the total number of withdrawals is stable.
    - Since the chain has migrated ⇒ any info related to deposits from L1 is stable.

#### v31 upgrade

**Step 0. Upgrade the ecosystem and deposit behavior**

The moment the ecosystem is upgraded, whenever the L1AssetTracker balance is increased or decreased, for each token we start tracking:

- `totalClaimedOnL1`
- `totalDepositedFromL1`
- If a token had any chainBalance prior to the upgrade, we just remember it as `preUpgradeChainBalance` for the pair of (chain, token).

**Step 1. Migrate balances to AssetTracker**

Note, that if we just assume that `L1AssetTracker.chainBalance` includes all deposits so far, then we are **wrong.** Since at the moment of the upgrade, all the previous deposits are actually inside *L1NativeTokenVault*. So whenever we are thinking about  `L1AssetTracker.chainBalance` and forget about migrating the token balance from L1NativeTokenVault, we are shooting ourselves in the foot.

Before any access to chainBalance it is better to require that the token balance has been migrated or automigrate.

**Step 2-3. Provide ETH token initial balance**

For zksync os chains no `totalSupply` is stored for the base token ATM. Starting from v31 upgrade, we’ll have the base token holder contract.

We use “2-3” as some chains might already start moving on GW, while others have still not set this value.

**Step 3.  The logic of token balance migration**

When the chain upgrades to v31, for each token we start tracking

- `totalWithdrawnToL1`
- `totalSuccessfulDeposits`
- We lazily obtain `totalPreV31TotalSupply` before doing anything that might affect the token’s total supply

Whenever we need to perform a migration of balance from L1 to GW, we send a message with the triple of `totalWithdrawnToL1` ,`totalSuccessfulDeposits` ,`totalPreV31TotalSupply`.

The amount of tokens to keep on L1 is:

```
preUpgradeChainBalance + totalWithdrawnToL1 + totalDepositedFromL1 - totalSuccessfulDeposits - preUpgradeTotalSupply - totalClaimedOnL1
```

 

- Explanation why the math is okay (why we dont even need any kinds of special migrations, conversions pausing or anything like that)
    
    The above should be okay, because what we want to keep on L1 is:
    
    ```
    all_withdrawals_to_l1 + all_failed_deposits_from_l1 - total_claimed
    ```
    
    Note, that `totalFailedDeposits = totalDepositedFromL1 - totalSuccessfulDeposits`
    
    Let's define:
    
    ```
    w1,...,wn -- all withdrawals
    d1,...,dm -- all deposits
    s1,...,st -- all successful deposits
    c1,...,cv -- all claims on L1.
    ```
    
    In essence we want to keep on L1 the following:
    
    ```
    (w1 + ... + wn) + (d1 + ... + dm) - (s1 + ... + st) - (c1 + ... + cv)
    ```
    
    `preUpgradeChainBalance` = `d1 + ... + d_m*` - `c1 + ... + cv*` for some m* and v*, i.e. for some segment right before the rest of the deposits and claims started being tracked separately.
    So by making `preUpgradeChainBalance + totalDepositedFromL1 - totalClaimedOnL1` we calculate the
    `(d1 + ... + dm) - (c1 + ... + cv)` part
    
    Similarly, `preUpgradeTotalSupply = (s1 + ... + st*) - (w1 + ... + wn*)` and by adding the values after the upgrade we get `(s1 + ... + st) - (w1 + ... + wn)`
    

> Implementation note: all the properties on L1 discussed above: chainBalance,totalSupply, preUpgradeChainBalance, states, etc are all for pairs (chain, token) and for “token” as a global entity. If a token has been present on 5 chains prior to v31, each chain will have to go through the procedure.
> 

> ⚠️ Important note: ZKsync OS chains do NOT have totalSupply for the base token provided by default. So the base token migration is only possible after the chain has migrated.

> A small note: we assume that the set of deposits that if failed may be claimed from the chain's balance directly and not its settlement layer is the same as the set of deposits that were sent when the chain settled on L1. I.e. we rely on the deposit invariant (see [Disabling deposits during migrations](../../gateway/chain_migration.md#deposit-pausing)).

In order for information to flow smoothly, migrating to the Gateway has the following steps:

- `L2AssetTracker.initiateL1ToGatewayMigrationOnL2` is called by any user. This function obtains the current balance migration data of the token (including `totalWithdrawalsToL1`, `totalSuccessfulDepositsFromL1`, and `totalPreV31TotalSupply`) and sends it to L1.
- The `L1AssetTracker` will receive the message and update the `chainBalance` mapping of the Chain and the Gateway, and forwards the message to GW as well as the L2.
- The Gateway's `AssetTracker` will receive the message and update the `chainBalance` mapping.
- The L2 `AssetTracker` will receive the message and update the `assetMigrationNumber`, enabling withdrawals and interop.

#### Migration from Gateway

On Gateway all withdrawals are processed in the `processLogsAndMessages` function. This means that the `chainBalance` mapping will match the totalSupply of the token on the chain. This means that the whole balance of the chain can be migrated to L1. The steps are accordingly:

- The migration is initiated on the Gateway via `GWAssetTracker.initiateGatewayToL1MigrationOnGateway` function, the balance is balance to be migrated is sent back to L1.
- The `L1AssetTracker` will receive the message and increase the `chainBalance` of the Chain and decrease the balance of the Gateway.

In the future the following scheme could be supported:
- If a chain does not settle on ZK Gateway, anyone is able to call `GWAssetTracker.initiateGatewayToL1MigrationOnGateway` as many times as anyone likes and the balance can be migrated to L1 as many times as needed to ensure that any incoming interops could be received.

In order to implement the logic above, we would have to manage replay protection properly. In this release, as a temporary simplification, the system allows the chain's balance to be withdrawn to L1 *once* and only after the chain has migrated from ZK Gateway. For now, we use `assetMigrationNumber` as the replay protection mechanism until a better version is implemented.

#### Disabling deposits during migrations

Let's recall the deposit invariant from [here](#failed-deposits): all deposits that are processed when the chain settles on  GW must be fully processed inside of it: they should be initiated when chain settles there as otherwise the chain would not be able to process failed deposit notifications.

It is forced inside `GWAssetTracker` and every deposit that the chain processed inside the batch must've went through GW first. So if a chain accidentally has a deposit unexecuted on L1 and needs to settle on GW, it wont be able to do so. It is the job of the chain's implementation (done inside `AdminFacet.forwardedBridgeBurn`) to ensure that.

But the above means that if a chain is permissionless, users could DDoS it with deposits never allowing a chain to actually migrate. To provide a solution suitable for permissionless chains, we added the ability to temporarily disable all incoming priority transactions: `AdminFacet.pauseDepositsBeforeInitiatingMigration`. More details on it are provided in the [chain migration doc](../../gateway/chain_migration.md#deposit-pausing).

#### Replay protection and edge cases with messaging

Our L2->L1 (or GW->L1) messages are valid in perpetuity. Typically, for actions like token withdrawals, we used `L1Nullifier` contract to store the nullifier that ensures that the same message can not be replayed twice.

However, for migration-related messages for the ease of implementation we used the `migrationNumber` and `assetMigrationNumber`: An asset-migrating message can be only processed on L1 once, since to process it, the carried `migrationNumber` needs to be greater than the current `assetMigrationNumber`.

#### Recovering from missed migrations

What happens if one did not migrate a tokens' balance?

- If a chain settles on L1, then only withdrawals wont be able to get finalized since `L1AssetTracker.chainBalance` of the chain would be too low.
- If a chain settles on GW, and it tries to make an outbound operation (e.g. withdrawal) then it wont be able to settle. It is the job of the chain's `L2AssetTracker` to ensure that no withdrawals can happen until the chain balance has been migrated to ZK Gateway.

### Deposit flow with L1AssetTracker

This section dives deeper into how `L1AssetTracker` is used to perform deposits for chains that settle on top of ZK Gateway. As already said before, when a deposit happens to the chain that settles on top of ZK Gateway, the balance accrual is assigned to ZK Gateway. However, the ZK Gateway also needs to increase the balance of the chain itself inside Gateway.

So when a message is relayed through ZK Gateway, ZK Gateway needs to query somehow how much funds were obtained by Gateway during the deposit. We could pass this data along during the deposit, but it is very hard to do so in a trustless manner without major changes to the codebase, it is the Mailbox of the L2 ZK chain that asks for the message to be relayed and this ZK chain may be potentially malicious and provide wrong values to the ZK Gateway.

Thus, it was decided that whenever a deposit happens, `L1AssetTracker` should provide an interface for the ZK Gateway to query the deposited data. The approach below is used:

1. When some token is accrued by a chain (`handleChainBalanceIncreaseOnL1` internal function), if a chain settles on top of ZK Gateway, we set in transient storage `_setTransientBalanceChange` the assetId and the amount deposited.
2. Later, the L1Bridgehub calls the Mailbox of the receiving L2 chain, which would then relay the message to the Gateway's mailbox by calling `requestL2TransactionToGatewayMailboxWithBalanceChange`.
3. Then, the ZK Gateway's Mailbox would call `L1AssetRouter.consumeBalanceChange` to "consume" this balance increase (i.e. reset it to 0, while reading the assetId and the amount). The ZK Gateway would then know the amount of funds and asset that should be attached with the message.

#### Security notes around the deposits

Firstly, it is important to note that in the current implementation, we trust the chain to provide the correct `baseTokenAmount` to relay, i.e. the transient store scheme from above is only used for consuming ERC20 deposits. This obviously means that only chains the L1 implementation of which ZK Gateway can trust are allowed to settle on ZK Gateway. This means that once ZK Gateway is added, no untrusted CTMs will be allowed at all or special protections will need to provided in the next release.

Next, it is important to discuss the edge cases around the transient storage. To avoid any double spending, the only way to read the transient values is to irreversibly consume them once. So regardless of any actions of malicious actors, a single balance increase will only be relayed to GW only once. From this invariant we know that the sum of chainbalances inside GW will never exceed GW's balance on L1.

However, the above opens doors for another potential error: someone maliciously consuming the variable for the chain. To prevent this, only whitelisted settlement layers are allowed to consume the balance. A second potential issue is overwrites: someone could overwrite the variable, but reentering and trying to make the deposit twice. This is prohibited since we use a `require` to ensure that the values are 0 (i.e. either unset or consumed) before new ones could be used.

### Withdrawal flow with L1AssetTracker

This section dives deeper into how `L1AssetTracker` is used to perform withdrawals for chains. Unlike deposits during which we definitely know the current settlement layer for the chain, a chain might've had a history of moving on and off from a settlement layer (including different ZK Gateways, L1, etc).

Withdrawals are always initiated from L1Nullifier (there are also some legacy methods that eventually end up calling L1Nullfier, so the principle is always the same). Similarly to deposits, where we needed to know the chain to increase the balance of (the chain itself or its settlement layer), we need to know which chain's balance to reduce. 

To get the chain id of the settlement layer for a particular withdrawal (or claimed deposit), we look at the proof for the message. You can read more about its recursive format [here](../../interop/message_root.md#proving-that-a-message-belongs-to-a-messageroot), but in a nutshell, a proof is a recursive structure that starts from the L2 chain's tree root and then (if chain settled on top of some Gateway), it ends with the GW's tree.

If the proof is of depth 1, i.e. it does not have any recursion, it means that the withdrawal belonged to the chain. If not, the withdrawal belonged to its settlement layer (the only exception is based on v31 upgrade, you can read more about in the sections above [here](#invariants-on-l1)). We ensure that all proofs have depth at most 2.

After verifying the correctness of the message, `L1Nullifier` stores inside `TRANSIENT_SETTLEMENT_LAYER_SLOT` and `TRANSIENT_SETTLEMENT_LAYER_SLOT+1` the settlement layer at the time of withdrawal and the batch number when withdrawal happened (the batch number would be needed for the checks against the time when the chain upgraded to v31).

To ensure secure verification, the `L1MessageRoot` is used (see [MessageRoot](../../interop/message_root.md)).

Then, when processing the decrease of the balance, the `L1AssetTracker` would read the values from above and decide which chain to reduce the balance of:
- If the withdrawal is from before the chain upgraded to v31, the chain is responsible.
- Otherwise, the settlement layer is responsible (or chain if it settled on L1).

#### Security notes around withdrawals

Unlike with deposits, we dont "consume" the transient stored values, i.e. it can always be read.

The invariant that is maintained is that right after the L1Nullifier is called, there can never be any untrusted calls until the `L1AssetTracker` is called (this ensures that nothing overwrites the previous values).

> Note, that it means that for now, only `NativeTokenVault` is supported in conjunction with the asset tracker as the asset handler must have trusted implementation.

Also, we say that the only way `handleChainBalanceIncreaseOnL1` can be called is that it must be preceded by a valid L1Nullifier call.

#### Claiming failed deposits

With regards to L1AssetRouter, the process of claiming failed deposits is extremely similar to withdrawals, i.e. the same `handleChainBalanceIncreaseOnL1` is called and the same invariants around L1Nullifier transient storage are expected.

### L1 and L2 native token registration

As discussed previously in the section about sum invariants ([see here](#total-sum-invariant)), to ensure that that sum of chainBalances for each asset is lower than `2^256-1`, all chain balances must originate from L1 (starting from `2^256-1` for origin chain at the start the first time). We will refer to the process of initializing the token's chainBalances for the first as "registration".

Note, that for this section we will only consider tokens that are interacted with for the first time starting from v31. To explore what the migration for pre-v31 assets looks like, check out the doc [here](../../upgrade_history/v31-bundles/upgrade_process_v31.md).

When a token is bridged for the first time, `registerNewToken` function is called, it would assign the max balance to the origin chain of the token. This function is only used in `L1AssetTracker` and `L2AssetTracker`.

#### L1AssetTracker

There are 2 ways how a token is registered on L1AssetRouter:
- For the first time through deposit. This is applicable for L1 native assets, these are only ones for which the first time of registration is deposit, during which `chainBalance[block.chainid][assetId]` is assigned to the maximal value (reduced by the deposited amount of course). 
- For the first time through withdrawals. This provides nice UX for L2-native assets that are withdrawn for the first time.

What if an origin chain for a token tries to settle on ZK Gateway? These chains would have to "migrate" their balance to Gateway in the [usual](#migrating-to-gateway) way. If the origin chain migrates for the first time, the `2^256-1` token balance will be minted for it. The UX consequence of the above is that before an L2-native token can be used for interop, it has to be registered on L1, which require a batch to be full settled on L1.

#### L2AssetTracker

The main job of the L2AssetTracker in this context is to ensure that if a chain settles on top of Gateway, it will not be able to withdraw until its balance has been either [migrated](#migrating-to-gateway) or it has been registered via the `registerL2NativeToken` explained above.

It is done by comparing `assetMigrationNumber` of the asset with the chain migration number and if those dont coincide, then not allowing to migrate.

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