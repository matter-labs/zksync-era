# Chain migration

## Ecosystem Setup

Chain migration reuses lots of logic from standard custom asset bridging which is enabled by the AssetRouter. The easiest way to imagine is that ZKChains are NFTs that are being migrated from one chain to another. Just like in case of the NFT contract, an CTM is assumed to have an `assetId := keccak256(abi.encode(L1_CHAIN_ID, address(ctmDeployer), bytes32(uint256(uint160(_ctmAddress)))))`. I.e. these are all assets with ADT = ctmDeployer contract on L1.

CTMDeployer is a very lightweight contract used to facilitate chain migration. Its main purpose is to serve as formal asset deployment tracker for CTMs. It serves two purposes:

- Assign bridgehub as the asset handler for the “asset” of the CTM on the supported settlement layer.

Currently, it can only be done by the owner of the CTMDeployer, but in the future, this method can become either permissionless or callable by the CTM owner.

- Tell bridgehub which address on the L2 should serve as the L2 representation of the CTM on L1. Currently, it can only be done by the owner of the CTMDeployer, but in the future, this method can become callable by the CTM owner.

![image.png](./img/ctm_gw_registration.png)

## The process of migration L1→GW

![image.png](./img/migrate_to_gw.png)

## Chain migration GW → L1

Chain migration from from L1 to GW works similar to how NFT bridging from L1 to another chain would work. Migrating back will use the same mechanism as for withdrawals.

Note, that for L2→L1 withdrawals via bridges we never provide a recovery mechanism. The same is the case with GW → L1 messaging, i.e. it is assumed that such migrations are always executable on L1.

You can read more about how the safety is ensured in the “Migration invariants & protocol upgradability” section.

![image.png](./img/migrate_from_gw.png)

## Chain migration GW_1 → GW_2

Currently we plan to only support a single whitelisted settlement layer, but if in the future more will be supported, as of now the plan is to migrate the chain firstly to L1 and then to GW.

## Chain migration invariants & protocol upgradability

Note, that once a chain migrates to a new settlement layer, there are two deployments of contracts for the same ZKChain.
For example if a chain migrates from L1 to GW, then there will be a deployment of the same ZKChain both on L1 and GW.
What’s more, the L1 part will always be used for priority transactions initiation.

There is a need to ensure that the chains work smoothly during migration and there are not many issues during the protocol upgrade.

You can read more about it [here](./gateway_protocol_upgrades.md).

## Deposit pausing

Our asset trackers depend on a fact that when a chain migrates to or from Gateway, there are no outstanding deposits. In order to ensure that, the chain admin needs to somehow stop deposits. One way could be to set `TransactionFilterer` contract. However, to make the approach more stage1-compatible in the future, the following, more permissionless approach has been chosen:

1. Firslty, the chain admin, needs to call `pauseDepositsBeforeInitiatingMigration`. It will give users a heads up of `PAUSE_DEPOSITS_TIME_WINDOW_START_MAINNET` time. Right now it is 3.5 days. I.e. for the next 3.5 days the users will still be able to conduct L1->L2 transactions. This is a headsup they get before the deposits are disabled.

2. After the headsup window, the deposits will be paused until `PAUSE_DEPOSITS_TIME_WINDOW_END_MAINNET` (right now it is 7 days) has elapsed since the deposit pausing has been announced (i.e. 3.5 days since the deposits have become paused).

3. To ensure that the chain has enough time to migrate and return, we give only 1.5 days to migrate to Gateway. 

4. Once the chain migrates or the migration fails, the user can proof the migration status and the `forwardedBridgeConfirmTransferResult` will be invoked, which will unpause the deposits regardless of the result.

All in all, the following imoprtant timestamps are used:

1. `s.pausedDepositsTimestamp` -- when chain admin has announced that the migration will happen soon and the deposits are paused.
2. `s.pausedDepositsTimestamp + PAUSE_DEPOSITS_TIME_WINDOW_START_MAINNET` = `s.pausedDepositsTimestamp + CHAIN_MIGRATION_TIME_WINDOW_START_MAINNET` -- when the deposits are actually paused and no more L1->L2 transactions can be accepted by the chain. Also, starting from this moment the chain can migrate to another settlement layers.
3. `s.pausedDepositsTimestamp + CHAIN_MIGRATION_TIME_WINDOW_END_MAINNET` -- when the chain can no longer migrate to GW (considered too close to end of the time window to risk it).
4. `s.pausedDepositsTimestamp + PAUSE_DEPOSITS_TIME_WINDOW_END_MAINNET` -- when the deposits are unpaused again and so the system gets back to normal.

During step (3) the deposits can be unlocked earlier if the ZK Gateway has processed the transactions soon enough.

> Important note, that the feature in its current form is NOT compatible with our stage1 design, since our stage1 design mandates much larger headsup for users to withdraw. This feature is mainly for future compatibility.

### Risks

The time-based solution above relies on:
- Admin being able to process all outstanding priority transactions within the `CHAIN_MIGRATION_TIME_WINDOW_END_MAINNET - PAUSE_DEPOSITS_TIME_WINDOW_START_MAINNET` (1.5 days right now) window. In case of severe spamming of the chain with L1->L2 transactions the chain can not have enough time to process all outstanding priority transacitons. The risk is deemed low, since processing priority transactions is much cheaper than including those.
- ZK Gateway not providing a valid confirmation of the migration success or failure within the `PAUSE_DEPOSITS_TIME_WINDOW_END_MAINNET - CHAIN_MIGRATION_TIME_WINDOW_END_MAINNET` (the last 1.5 days) window. Note, that not only Gateway needs to settle to provide this value, the users of the chain need to have the opportunity to confirm the result on L1 (so in theory might be subject to temporary L1 censorship). We assume that 1.5 days is enough.


If the chain *fails* to settle on settlement layer AND it receives an L1->L2 transaction that went through GW, the deposit will be attributed to ZK Gateway, while the deposit itself will fail, preventing funds from being released.

If the chain succeeded in its migration to ZK Gateway the early confirmation serves only to enable L1->L2 transactions again.

FIXME: the risks are insane we should update the logic.
