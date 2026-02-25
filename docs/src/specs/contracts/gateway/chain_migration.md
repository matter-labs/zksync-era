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

Chain migration from GW to L1 works similar to how NFT bridging from L1 to another chain would work. Migrating back will use the same mechanism as for withdrawals.

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

Asset trackers rely on the invariant that there are no outstanding L1->L2 deposits while a chain is migrating to or from Gateway.
To enforce this invariant, deposits are paused before migration and enable them only after the migration is complete.

### Flow

1. Chain admin calls `pauseDepositsBeforeInitiatingMigration`.
2. Deposits become paused after the `PAUSE_DEPOSITS_TIME_WINDOW_START`. In this release, since stage1 compatibility is not yet enabled for chains that settle on ZK Gateway, `PAUSE_DEPOSITS_TIME_WINDOW_START` is set to 0.
3. Migration can be initiated immediately after pause.
4. Deposits stay paused until the migration status is confirmed (`forwardedBridgeConfirmTransferResult`)

### Timestamps and checks

`s.pausedDepositsTimestamp` stores when pause was announced.

Deposits are considered paused when:

- `s.pausedDepositsTimestamp != 0`, and
- `s.pausedDepositsTimestamp + PAUSE_DEPOSITS_TIME_WINDOW_START <= block.timestamp`.

Migration is considered allowed when:

- `s.pausedDepositsTimestamp != 0`, and
- `s.pausedDepositsTimestamp + CHAIN_MIGRATION_TIME_WINDOW_START <= block.timestamp`.

Current constants set both start windows to `0`, so both conditions are immediate.

### Unpause behavior

- `forwardedBridgeConfirmTransferResult` unpauses deposits during migration result handling.
- `forwardedBridgeMint` also unpauses deposits on successful migration completion.
- `unpauseDeposits` remains available to the chain admin, but only if `isMigrationInProgress(chainId) == false`. This is only used to allow chains to have their deposits paused (and to migrate to Gateway) right after the chain is created.

### Stage1 note

Since stage1 is not yet supported for chains that settle on top of Gateway, in this release the delay before pausing deposits is 0. However, the code should be ready to be able to jump bump those constants in one of the future releases.

Additionally, there is a risk that the chain admin may abuse this functionality by disabling deposits to prevent users from executing any deposits, while actually not even trying to migrate to ZK Gateway. It is a known issue and will be resolved in one of the future upgrades. Right now it is considered acceptable, since:
- A malicious chain admin can set `transactionFilterer` that would achieve the same goal anyway.
- This functionality (as well as the `transactionFilterer` one) is disabled for chains that aim to support stage1, i.e. `s.priorityModeInfo.canBeActivated = true`. 

### Conclusion

If a chain uses our DiamondProxy implementation, then it is enforced that when the chain starts its migration to L1:
- The deposits have been paused + no priority transactions are left unprocessed. 
- The only way to enable those back is to provide the proof that the migration has either succeeded or failed.

When it migrates back from GW similarly we enforce that the deposits have been paused + no priority transactions are left unprocessed. It is assumed that GW->L1 migration never fails and so the only way the deposits will be enabled is after the chain completes its migration to L1.

Both of the situations above perfectly ensure that the deposit invariant that is required for asset migration holds even in the case of a malicious chain admin.

## Risk model update

In case of a stuck and uncooperative ZK Gateway deposits can remain paused indefinitely if no confirmation is provided and admin does not manually unpause. However, it is assumed that a chain that migrates on top of ZK Gateway trusts it.
