# Upgrade process for v31

## Intro and prerequisites

This document explores what the upgrade to v31 will look like for chains. This document assumes your overall
understanding of our system prior to v29 and what the system looks like after v31 (note V30 is skipped on Era, as it is
used on ZKSync OS).

Especially the following documents must be read first:

- [Asset Tracker](../../contracts/bridging/asset_tracker/asset_tracker.md). This document is especially important as it
  explores what are the expected state of the system prior to v31 upgrade.
- [Message Root](../../contracts/interop/message_root.md).

Quick recap on what the system is expected to look like when v31 reaches mainnet:

- The ecosystem has only one deprecated ZK Gateway and it is Era based. Only Era-based chains had migrated there. So one
  can assume that nothing malicious has happened on Gateway.
- All the chains that once settled on Gateway have migrated back to L1. No chains may be settling on top of Era based ZK
  Gateway (we plan to [migrate them back to L1](#stopping-deposits-for-zk-gateway) before the upgrade starts).
- On L1 a separate ZKSync OS CTM is deployed. This CTM is initially controlled by a multisig, but ownership will be
  transferred to a decentralized governance prior to the V31 upgrade. Before ownership is transferred, the ZKSync OS
  chains must be treated as potentially totally malicious with regard to Era based chains.

What future state of the system should v31 support:

- A new ZKSync OS based Gateways.
- Note, that by the time of the v31 upgrade ZKsync OS chains will be controlled by the decentralized governance.

## Ecosystem Upgrade process

The biggest complexities of the upgrade process are:

- Ensuring that for past withdrawals we know whether to deduct from `chainBalance` of the chain vs chain balance of its
  settlement layer.
- Ensuring that assets that were bridged before oblige to the same invariants that new assets will
  ([see here](../../contracts/bridging/asset_tracker/asset_tracker.md#total-sum-invariant)) in the L1AssetTracker.

The overall upgrade will consist of the usual three steps:

- **Stage0.** We pause all migrations to the ZK Gateway. This is just a standard step done during every upgrde. Almost
  nothing changes at this point, except for the fact that chains wont be able to change their settlement layer. Note,
  that we will also [ensure](#stopping-deposits-for-zk-gateway) that no chains are settling on top of ZK Gateway at the
  time of this upgrade, and Gateway will be deprecated.
- **Stage1.** We upgrade all contracts on L1 for the Core contracts, and both CTMs. Usually this is when the CTM inside
  ZK Gateway is also upgraded (via L1->GW transaction as usual). However at this point the EraVM Gateway will be
  deprecated. Will be done ~2 days after the previous stage.
- **Stage2.** We unblock migrations, will be done very soon after the previous stage.

The `SettlementLayerV31Upgrade` will be the upgrade implementation for each chain.

#### Stopping deposits for ZK Gateway

To successfully upgrade ZK Gateway, we need it to process all previous relayed deposits (i.e. not coming to ZK Gateway
as _a chain_, but as _a settlement layer_). This is needed to maintain the invariants around deposits (see
[Disabling deposits during migrations](../../contracts/bridging/asset_tracker/asset_tracker.md#disabling-deposits-during-migrations)).
In reality, it is very hard to distinguish between relayed deposits and the normal ones, so we just demand that it
processed all priority transactions.

The easiest way to ensure that the condition above holds is to put a requirement that at the time of `stage0` all chains
have migrated to L1, and to deprecate the EraVM Gateway. Deprecation will happen with a new `TransactionFilterer` that
would ensure that no chain is allowed to migrate on top of ZK Gateway.

## Chain balance migration

### When settling on L1

Right after stage1 of the v31 upgrade, the bridging contracts will start using `L1AssetTracker` to track users balances.
So balances of chains will have to be migrated to allow finalizing withdrawals.

To migrate a balance for a token, a chain can call `migrateTokenBalanceFromNTVV31`. During such migration, the chain
balance mapping is zeroed out on L1NativeTokenVault and given to the corresponding chain inside L1AssetTracker. During
the entire process we ensure that the
[sum invariant](../../contracts/bridging/asset_tracker/asset_tracker.md#total-sum-invariant) is withheld. If the total
sum of chain balance inside L1NTV for some token is larger than 2^256-1, chains wont be able to migrate its balance to
their `L1AssetTracker` balance.

#### Security note on malicious token

Note, that since the sum invariant started to be held only starting from v31, it is possible that the sum of token
balances for a token is larger than 2^256-1.

One note is that all tokens that originated from L2 are limited by the `totalSupply` variable on L1 inside the bridged
token, tokens native to L2 can not have total sum of balances on L1 larger than `uint256(max)`.

Tokens that originated from L1 however, could in theory deposit `2^256-1` tokens to multiple chains. When a chain
migrates on top of a ZK Gateway, these tokens wont be usable to interop or withdrawals, since unless the chain managed
to migrate the needed balance via `migrateTokenBalanceFromNTVV31`, the chains balance would be 0 and it Gateway wont
allow to use this token.

In any case, all the invariants that are expected from a token will be preserved and only the users of such tokens will
be affected and not the chain itself.

### `L2AssetTracker`

Inside `L2AssetTracker` we only track balances for chain itself to prevent it withdrawing more than `2^256-1` in total
for each token. All tokens should start with empty balance inside `L2AssetTracker`.

If a token is registered for the first time (after v31), we can just set `2^256-1` balance for it automatically.
However, if a token has existed before, the situation is trickier, since we never tracked the total withdrawn funds
before. We will use the heuristic of assigning `token.balanceOf(L2_NATIVE_TOKEN_VAULT)` as the initial total withdrawn
funds so far.

This heuristic is correct for usual assets as whenever a token is withdrawn, it is escrowed inside
`L2_NATIVE_TOKEN_VAULT` (increasing its balance). The token could be also sent directly to the contract, but these are
unaccessible anyway.

However, if a token has a malicious or unusual implementation, the situation gets tricker:

- If we get an overly small number of tokens withdrawn so far (i.e. its `chainBalance` inside `L2AssetTracker` is bigger
  than it should be). This means that a chain can spawn withdrawals such that the total sum of potential withdrawals is
  larger than `2^256-1`. However, it does not impact chain settlement while it is on L1. In case a chain settles on top
  of a Gateway, the chain would need to
  [migrate](../../contracts/bridging/asset_tracker/asset_tracker.md#migrating-and-settling-on-gateway) these balances
  first. The migration process would fail, since the balance that the chain claims to have is smaller than what it would
  have on L1. And so no external interactions would be possible with such a token, but the chain itself will be secure
  and will settle correctly.
- If we get an overly large number of tokens withdrawn so far (i.e. its `chainBalance` inside `L2AssetTracker` is
  smaller than it should be). Then the token will be withdrawable or interopable up to the point when the
  `L2AssetTracker.chainBalance` becomes 0.

In all of the cases above, the impact is on the token only and not the chain. Considering how rare such cases are, it is
acceptable.

## Populating batch info inside `MessageRoot`

Starting from v31 the MessageRoot contract should be the source of truth for latest chain batch number and chain batch
root. These values need to be populated.

### During message root upgrade

When `MessageRoot` is upgraded, all the chains that have been existing so far will get their
`v31UpgradeChainBatchNumber` populated with either `V31_UPGRADE_CHAIN_BATCH_NUMBER_PLACEHOLDER_VALUE_FOR_GATEWAY` or
`V31_UPGRADE_CHAIN_BATCH_NUMBER_PLACEHOLDER_VALUE_FOR_L1` depending on wheere the chain settled at the time of the
upgrade. Then the chains will populate these values as they upgrade, which is explained in the following sections.

### Expected chain upgrade behavior

A chain is expected to call `saveV31UpgradeChainBatchNumber` at the time when it upgrades to v31 on its settlement
layer. It must have all of its committed batches executed on the settlement layer.

The `MessageRoot` would query the chain for the total batches executed at that time and will write it down inside the
`MessageRoot`. It will populate both `v31UpgradeChainBatchNumber` and `currentChainBatchNumber`.

Whenever a chain will change its settlement layer, the data will be moved to the message root on the new settlement
layer. From this point the behavior should be the same as for chains that were spawned with v31 initially.

### Notes on malicious behavior

Before upgrade ZKsync OS powered chains are already expected to exist. They may be controlled by a development multisig
and so we need to take into account that they may provide malicious values there for the current executed batch.

However, in such case they will only make life harder for themselves: when they will move to a Gateway, only batches
that have numbers started from `v31UpgradeChainBatchNumber` will be accepted (since then ZK Gateway starts being
responsible for all withdrawals that come from a batch).

Also, when verifying that message from a chain with batch number higher than `v31UpgradeChainBatchNumber` that used a
settlement layer we will always double check that the settlement layer agreed to the batch.

Additionally, before ZKsync OS chains will even be able to move to a ZKsync OS powered settlement layer, the ZKsync OS
CTM will be moved under the control of the decentralized governance, so during the vote to accept the ownership everyone
will have plenty of time to double check that no malicious behavior ever happened while the CTM was controlled by a
development wallet.
