# Gateway protocol versioning and upgradability

One of the hardest part about gateway (GW) is how do we synchronize interaction between L1 and L2 parts that can potentially have different versions of contracts. This synchronization should be compatible with any future CTM that may be present on the gateway.

Here we describe various scenarios of standard/emergency upgrades and how will those play out in the gateway setup.

## General idea

We do not enshrine any particular approach on the protocol level of the GW. The following is the approach used by the standard Era CTM, which also manages GW.

Upgrades will be split into two parts:

- “Inactive chain upgrades” ⇒ intended to update contract code only and not touch the state or touch it very little. The main motivation is to be able to upgrade the L1 contracts without e.g. adding new upgrade transactions.
- “Active chain upgrades” ⇒ same as the ones that we have today: full-on upgrade that also updates bootloader, insert system upgrade transaction and so on.

In other words:

`active upgrade = inactive upgrade + bootloader changes + setting upgrade tx`

The other difference is that while “active chain upgrades” are usually always needed to be forced in order to ensure that contracts/protocol are up to date, the “inactive chain upgrades” typically involve changes in the facets’ bytecode and will only be needed before migration is complete to ensure that contracts are compatible.

To reduce the boilerplate / make management of the upgrades easier, the abstraction will be basically implemented at the upgrade implementation level, that will check `if block.chainid == s.settlementLayer { ... perform active upgrade stuff } else { ... perform inactive upgrade stuff, typically nothing m}.`

## Lifecycle of a chain

While the chain settles on L1 only, it will just do “active chain upgrades”. Everything is the same as now.

When a chain starts its migration to a new settlement layer (regardless of whether it is gateway or not):

1. It will be checked the that the protocolVersion is the latest in the CTM in the current settlement layer (just in case to not have to bother with backwards compatibility).
2. The `s.settlementLayer` will be set for the chain. Now the chain becomes inactive and it can only take “inactive” upgrades.
3. When migration finishes, it will be double checked that the `protocolVersion` is the same as the one in the target chains’ CTM.

If the chain has already been deployed there, it will be checked that the `protocolVersion` of the deployed contracts there is the same as the one of the chain that is being moved. 4. All “inactive” instances of a chain can receive “inactive” upgrades of a chain. The single “active” instance of a chain (the one on the settlement layer) can receive only active upgrades.

In case step (3) fails (or for any other reason the chain fails), the migration recovery process should be available. (`L1AssetRouter.bridgeRecoverFailedTransfer` method). Recovering a chain id basically just changing its `settlementLayerId` to the current block.chainid. It will be double checked that the chain has not conducted any inactive upgrades in the meantime, i.e. the `protocolVersion` of the chain is the same as the one when the chain started its migration.

In case we ever do need to do more than simply resetting `settlementLayerId` for a chain in case of a failed migration, it is the responsibility of the CTM to ensure that the logic is compatible for all the versions.

## Stuck state for L1→GW migration

The only unrecoverable state that a chain can achieve is:

- It tries to migrate and it fails.
- While the migration has happening an “inactive” upgrade has been conducted.
- Now recovery of the chain is not possible as the “protocol version” check will fail.

This is considered to be a rare event, but it will be strongly recommended that before conducting any inactive upgrades the migration transaction should be finalized.

In the future, we could actively force it, i.e. require confirmation of a successful migration before any upgrades on a migrated chain could be done.

## Safety guards for GW→L1 migrations

Migrations from GW to L1 do not have any chain recovery mechanism, i.e. if the step (3) from the above fails for some reason (e.g. a new protocol version id is available on the CTM), then the chain is basically lost.

### Protocol version safety guards

- Before a new protocol version is released, all the migrations will be paused, i.e. the `pauseMigration` function will be called by the owner of the Bridgehub on both L1 and L2. It should prevent migrations happening in the risky period when the new version is published to the CTM.
- Assuming that no new protocol versions are published to CTM during the migration, the migration must succeed, since both CTM on GW and on L1 will have the same version and so the checks will work fine.
- The finalization of any chain withdrawal is permissionless and so in the short term the team could help finalize the outstanding migrations to prevent funds loss.

> The approach above is somewhat tricky as it requires careful coordination with the governance to ensure that at the time of when the new protocol version is published to CTM, there are no outstanding migrations.

In the future we will either make it more robust or add a recovery mechanism for failed GW → L1 migrations.

>

### Batch number safety guards

Another potential place that may lead for a chain to not be migratable to L1 is if the number of outstanding batches is very high, which can lead to migration to cost too much gas and being not executable no L1.

To prevent that, it is required for chains that migrate from GW that all their batches are executed. This ensures that the number of batches’ hashes to be copied on L1 is constant (i.e. just 1 last batch).

## Motivation

The job of this proposal is to reduce the number of potential states in which the system can find itself in to a minimum. The cases that are removed:

- Need to be able to migrate to a chain that has contracts from a different protocol version
- Need to be able for CTM to support migration of chains with different versions. Only `bridgeRecoverFailedTransfer` has to be supported for all the versions, but its logic is very trivial.

The reason why we can not conduct “active” upgrades everywhere on both L1 and L2 part is that for the settlement layer we need to write the new protocol upgrade tx, while NOT allowing to override it. On other hand, for the “inactive” chain contracts, we need to ignore the upgrade transaction.

## Forcing “active chain upgrade”

For L1-based chains, forcing those upgrades will work exactly same as before. Just during `commitBatches` the CTM double checks that the protocol version is up to date.

The admin of the CTM (GW) will call the CTM (GW) with the new protocol version’s data. This transaction should not fail, but even if it does fail, we should be able to just re-try. For now, the GW operator will be trusted to not be malicious

### Case of malicious Gateway operator

In the future, malicious Gateway operator may try to exploit a vulnerability in an CTM.

The recommended approach here is the following:

- Admin of the CTM (GW) will firstly commit to the upgrade (for example, preemptively freeze all the chains).
- Once the chains are frozen, it can use L1→L2 communication to pass the new protocol upgrade to CTM.

> The approach above basically states that “if operator is censoring, we’ll be able to use standard censorship-resistance mechanism of a chain to bypass it”. The freezing part is just a way to not tell to the world the issue before all chains are safe from exploits.

It is the responsibility of the CTM to ensure that all the supported settlement layers are trusted enough to uphold to the above protocol. Using any sort of Validiums will be especially discouraged, since in theory those could get frozen forever without any true censorship resistance mechanisms.

Also, note that the freezing period should be long enough to ensure that censorship resistance mechanisms have enough time to kick in

## Forcing “inactive chain upgrade”

Okay, imagine that there is a bug in an L1 implementation of a chain that has migrated to Gateway. This is a rather rare event as most of the action happens on the settlement layer, together with the ability to steal the most of funds.

In case such situation does happen however, the current plan is just to:

- Freeze the ecosystem.
- Ask the admins nicely to upgrade their implementation. Decentralized token governance can also force-upgrade those via CTM on L1.

## Backwards compatibility

With this proposal the protocol version on the L1 part and on the settlement layer part is completely out of sync. This means that all new mailboxes need to support both accepting and sending all versions of relayed (L1 → GW → L2) transactions.

For now, this is considered okay. In the future, some stricter versioning could apply.

## Notes

### Regular chain migration moving chain X from Y to Z (where Y is Z’s settlement layer)

So assume that Y is L1, and Z is ‘Gateway’.

Definition:

`ZKChain(X)` - ‘a.k.a ST / DiamondProxy’ for a given chain id X

`CTM(X)` - the State transition manager for a given chain id X

1. check that `ZKChain(X).protocol_version == CTM(X).protocol_version` on chain Y.
2. Start ‘burn’ process (on chain Y)
   1. collect `‘payload’` from `ZKChain(X)` and `CTM(X)` and `protocol_version` on chain Y.
   2. set `ZKChain(X).settlement_layer` to `address(ZKChain(Z))` on chain Y.
3. Start ‘mint’ process (on chain Z)
   1. check that `CTM(X).protocol_version == payload.protocol_version`
   2. Create new `ZKChain(X)` on chain Z and register in the local bridgehub & CTM.
   3. pass `payload` to `ZKChain(X)` and `CTM(X)` to initialize the state.
4. If ‘mint’ fails - recover (on chain Y)
   1. check that `ZKChain(X).protocol_version == payload.protocol_version`
      1. important, here we’re actually looking at the ‘HYPERCHAIN’ protocol version and not necessarily CTM protocol version.
   2. set `ZKChain(X).settlement_layer` to `0` on chain Y.
   3. pass `payload` to `IZKChain(X)` and `CTM(X)` to initialize the state.

### ‘Reverse’ chain migration - moving chain X ‘back’ from Z to Y

(moving back from gateway to L1).

1. Same as above (check protocol version - but on chain Z)
2. Same as above (start burn process - but on chain Z)
3. Same as above (start ‘mint’ - but on chain Y)
   1. same as above
   2. creation is probably not needed - as the contract was already there in a first place.
   3. same as above - but the state is ‘re-initialized’
4. Same as above - but on chain ‘Z’
