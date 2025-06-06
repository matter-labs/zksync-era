# The upgrade process to the new version

Gateway system introduces a lot of new contracts and so conducting so to provide the best experience for ZK chains the
multistage upgrade will be provided. The upgrade will require some auxiliary contracts that will exist only for the
purpose of this upgrade.

## Previous version

The previous version can be found [here](https://github.com/matter-labs/era-contracts/tree/main).

The documentation for the previous version can be found [here](https://github.com/code-423n4/2024-03-zksync).

However, deep knowledge of the previous version should not be required for understanding. But this document _does_
require understanding of the new system, so it should be the last document for you to read.

## Overall design motivation

During design of this upgrade we followed two principles:

- Trust minimization. I.e. once the voting has started, no party can do damage to the upgrade or change its course. For
  instance, no one should be able to prevent a usable chain from conducting the upgrade to the gateway.
- Minimal required preparation for chains. All of the required contracts for the upgrade (e.g. rollup L2DA validator,
  etc) will be deployed during the upgrade automatically. This allows to minimize risks of mistakes for each individual
  chain.

There are four roles that will be mentioned within this document:

- “Governance” — trusted entity that embodies the whole
  [decentralized voting process for ZK chain ecosystem](https://blog.zknation.io/introducing-zk-nation/).
- “Ecosystem admin” — relatively trusted role for non-critical operation. It will be typically implemented as a
  multisig, that is approved by the governance and can only do limited operations to facilitate the upgrade. It can not
  alter the content of the upgrade nor it should be able to somehow harm chains by weaponizing the upgrade.
- “Chain admin” — an admin of a ZK chain. An entity with limited ability to govern their own chain (they can choose who
  are the validators of the chain, but can not change the general behavior of the their chain).
- “Deployer”. This role may not be mentioned, but it is implicitly present during the preparation stage. This is a hot
  wallet that is responsible for deploying the implementation of new contracts, etc. The governance should validate all
  its actions at the start of the voting process and so no trust is assumed from this wallet.

## Ecosystem preparation stage

This stage involves everything that is done before the voting starts. At this stage, all the details of the upgrade must
be fixed, including the chain id of the gateway.

More precisely, the implementations for the contracts will have to be deployed. Also, all of the new contracts will have
to be deployed along with their proxies, e.g. `CTMDeploymentTracker`, `L1AssetRouter`, etc. Also, at this stage, the
bytecodes of L2 contracts are considered fixed, i.e. they should not change.

### Ensuring Governance ownership

Some of the new contracts (e.g. `CTMDeploymentTracker` ) have two sorts of admins: the admin of the their proxy as well
the `owner` role inside the contract. Both should belong to governance (the former is indirectly controlled by
governance via a `ProxyAdmin` contract).

The governance needs to know that it will definitely retain the ownership of these contracts regardless of the actions
of their deployer. There are multiple ways this is ensured:

- For `TransparentUpgradaeableProxy` this is simple: we can just transfer the ownership in one step to the `ProxyAdmin`
  that is under control of the governance.
- For contracts that are deployed as standalone contracts (not proxies), then if we possible we provide the address of
  the owner of in the constructor.
- For proxies and for contracts for which transferring inside the constructor is not option, we would transfer the
  ownership to a `TransitionaryOwner` contract. This is a contract that is responsible for being a temporary owner until
  the voting ends and it can do only two things: accept ownership for a contract and atomically transfer it to the
  governance. This is a workaround we have to use since most of our contracts implement `Ownable2Step` and so it is not
  possible to transfer ownership in one go.

PS: It may be possible that for more contracts, e.g. some of the proxies we could’ve avoided the `TransitionaryOwner`
approach by e.g. providing the governance address inside of an initializer. But we anyway need the `TransitionaryOwner`
for `ValidatorTimelock`, so we decided to use it in most places to keep the code simpler. Also, some contracts will use
`Create2AndTransfer` contract that deploys a contract and immediately transfers ownership to the governance.

### L2SharedBridge and L2WETH migration

In the current system (i.e. before the gateway upgrade), the trusted admin of the L1SharedBridge is responsible for
[setting the correct L2SharedBridge address for chains](https://github.com/matter-labs/era-contracts/blob/aafee035db892689df3f7afe4b89fd6467a39313/l1-contracts/contracts/bridge/L1SharedBridge.sol#L249)
(note that the links points to the old code). This is done with no additional validation. The system is generally
designed to protect chains in case when a malicious admin tries to attack a chain. There are two measures to do that:

- The general assumption is that the L2 shared bridge is set for a chain as soon as possible. It is a realistic
  assumption, since without it no bridging of any funds except for the base token is possible. So if at an early stage
  the admin would put a malicious l2 shared bridge for a chain, it would lose its trust from the community and the chain
  should be discarded.
- Admin can not retroactively change L2 shared bridge for any chains. So once the correct L2 shared bridge is set, there
  is no way a bad admin can harm the chain.

The mapping for L2SharedBridge will be used as a source for the address of `L2SharedBridgeLegacy` contract address
during the migration.

To correctly initialize the `L2NativeTokenVault` inside the gateway upgrade, we will need the address of the L2 Wrapped
Base Token contract
[as well](https://github.com/matter-labs/era-contracts/blob/main/l2-contracts/contracts/bridge/L2WrappedBaseToken.sol)
(note that the link is intentionally for the current codebase to show that these are deployed even before the upgrade).

The data to execute the upgrade with is gathered on L1, so we need to create a mapping on L1 from
`chainId => l2WrappedBaseToken`. This is what the `L2WrappedBaseTokenStore` contract for.

Some chains already have `L2WrappedBaseToken` implementation deployed. It will be the job of the admin of the contract
to prepopulate the contract with the correct addresses of those. The governance will have to double check that for the
existing chains this mapping has been populated correctly before proceeding with the upgrade.

Since we do not want to stop new chain creation while the voting is in progress, the admin needs to have the ability to
add both new `L2SharedBridges` and the new `L2WrappedBaseToken` addresses to the mappings above. The following
protections are put in place:

- In case the trusted admin maliciously populated the addresses for any chains that were created before the voting
  started, the governance should just reject the voting
- In case the trusted admin maliciously populated the addresses for a chain after the voting has ended, the same
  assumptions as the ones described for L2SharedBridge apply, i.e. the chain should have its `L2SharedBridge` and
  `L2WrappedBaseToken` deployed asap after the creation of the chain, in case the admin did something malicious, they
  should immediately discard the chain to prevent loss of value.

### Publishing bytecodes for everyone

Before a contract can be deployed with a bytecode, it must be marked as “known”. This includes system contracts. This
caused some inconveniences during previous upgrades:

- For each chain we would have to publish all factory dependencies for the upgrade separately, making it expensive and
  risk-prone process.
- If a chain forgets to publish bytecodes for a chain before it executes an upgrade, there is little way to recover
  without intervention from the Governance.

This upgrade the different approach is used to ensure safe and riskless preparation for the upgrade:

- All bytecodes that are needed for this upgrade must be published to the `BytecodesSupplier` contract.
- The protocol upgrade transaction will have all the required dependencies in its factory deps. During the upgrade they
  will be marked as known automatically by the system. The operator of a chain needs to grab the preimages for those
  from events emitted by the `BytecodesSupplier`.
- It will be the job of the governance to verify that all the bytecodes were published to this contract.

## Voting stage

### Things to validate by the governance

- The L1/L2 bytecodes are correct and the calldata is correct.
- That the correct L2SharedBridge are populated in L1SharedBridge (note that it is a legacy contract from the current
  system that becomes L1Nullifer in the new upgrade) and that L2WrappedBaseTokenStore has been populated correctly.
- [That the ownership is correctly transferred to governance.](#ensuring-governance-ownership)
- That the bytecodes were published correctly to the `BytecodeSupplier` contract.

### Things to sign by the governance

The governance should sign all operations that will happen in all of the consecutive stages at this time. There will be
no other voting. Unless stated otherwise, all the governance operations in this document are listed as dependencies for
one another, i.e. must be executed in strictly sequential order.

## Stage 1. Publishing of the new protocol upgrade

### Txs by governance (in one multicall)

1. The governance accepts ownership for all the contracts that used `TransitionaryOwner`.
2. The governance publishes the new version by calling `function setNewVersionUpgrade`.
3. The governance sets the new `validatorTimelock`.
4. The governance calls `setChainCreationParams` and sets the new chain creation params to ensure that the chain
   creation fails.
5. The governance should call the `GovernanceUpgradeTimer.startTimer()` to ensure that the timer for the upgrade starts.
   It will give the ecosystem's chains a fixed amount of time to upgrade their chains before the old protocol version
   becomes invalid.

### Impact

The chains will get the ability to upgrade to the new protocol version. They will be advised to do so before the
deadline for upgrade runs out.

Also, new chains wont be deployable during this stage due to step (4).

Chains, whether upgraded or not, should work as usual as the new L2 bridging ecosystem is fully compatible with the old
L1SharedBridge.

## Chain Upgrade flow

Let’s take a deeper look at how upgrading of an individual chain would look like.

### Actions by Chain Admins

As usual, the ChainAdmin should call `upgradeChainFromVersion`. What is unusual however:

- ValidatorTimelock changes and so the admin should call the new ValidatorTimelock to set the old validators there.
- The new DA validation mechanism is there and so the ChainAdmin should set the new DA validator pair.
- If a chain should be a permanent rollup, the ChainAdmin should call the `makePermanentRollup()` function.

It is preferable that all the steps above are executed in a multicall for greater convenience, though it is not
mandatory.

This upgrade adds a lot of new chain parameters and so these
[should be managed carefully](../../contracts/chain_management/admin_role.md).

### Upgrade flow in contracts

Usually, we would perform an upgrade by simply doing a list of force deployments: basically providing an array of the
contracts to deploy for the system. This array would be constant for all chains and it would work fine.

However in this upgrade we have an issue that some of the constructor parameters (e.g. the address of the
`L2SharedBridgeLegacy`) are specific to each chain. Thus, besides the standard parts of the upgrades each chain also has
`ZKChainSpecificForceDeploymentsData` populated. Some of the params to conduct those actions are constant and so
populate the `FixedForceDeploymentsData` struct.

If the above could be composed on L1 to still reuse the old list of `(address, bytecodeHash, constructorData)` list,
there are also other rather complex actions such as upgrading the L2SharedBridge to the L2SharedBridgeLegacy
implementation that require rather complex logic.

Due to the complexity of the actions above, it was decided to put all those into the
[L2GatewayUpgrade](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/L2GatewayUpgrade.sol)
contract. It is supposed to be force-deployed with the constructor parameters containing the
`ZKChainSpecificForceDeploymentsData` as well as `FixedForceDeploymentsData`. It will be forcedeployed to the
ComplexUpgrader’s address to get the kernel space rights.

So most of the system contracts will be deployed the old way (via force deployment), but for more complex thing the
`L2GatewayUpgrade` will be temporarily put onto `ComplexUpgrader` address and initialize additional contracts inside the
constructor. Then the correct will be put back there.

So entire flow can be summarized by the following:

1. On L1, when `AdminFacet.upgradeChainFromVersion` is called by the Chain Admin, the contract delegatecalls to the
   [GatewayUpgrade](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/upgrades/GatewayUpgrade.sol)
   contract.
2. The `GatewayUpgrade` gathers all the needed data to compose the `ZKChainSpecificForceDeploymentsData`, while the
   `FixedForceDeploymentsData` is part is hardcoded inside the upgrade transaction.
3. The combined upgrade transaction consists of many forced deployments (basically tuples of
   `(address, bytecodeHash, constructorInput)`) and one of these that is responsible for the temporary
   `L2GatewayUpgrade` gets its `constructorInput` set to contain the `ZKChainSpecificForceDeploymentsData` /
   `FixedForceDeploymentsData`.
4. When the upgrade is executed on L2, it iterates over the forced deployments, deploys most of the contracts and then
   executes the `L2GatewayUpgrade`.
5. `L2GatewayUpgrade` will deploy the L2 Bridgehub, MessageRoot, L2NativeTokenVault, L2AssetRouters. It will also deploy
   l2WrappedBaseToken if missing. It will also upgrade the implementations the L2SharedBridge as well as the
   UpgradaeableBeacon for these tokens.

## Stage 2. Finalization of the upgrade

### Txs by governance (in one multicall)

- call the `GovernanceUpgradeTimer` to check whether the deadline has passed as only after that the upgrade can be
  finalized.
- set the protocol version deadline for the old version to 0, i.e. ensuring that all the chains with the old version
  wont be able to commit any new batches.
- upgrade the old contracts to the new implementation.
- set the correct new chain creation params, upgrade the old contracts to the new one

### Txs by anyone

After the governance has finalized the upgrade above, anyone can do the following transactions to finalize the upgrade:

For each chainId:

- `Bridgehub.setLegacyBaseTokenAssetId`
- `Bridgehub.setLegacyChainAddress`

For each token:

- register token inside the L1NTV

For each chain/token pair:

- update chain balances from shared bridge for L1NTV

The exact way these functions will be executed is out of scope of this document. It can be done via a trivial multicall.

### Impact

The ecosystem has been fully transformed to the new version.

## Security notes

### Importance of preventing new batches being committed with the old version

The new `L1AssetRouter` is not compatible with chains that do not support the new protocol version as they do not have
`L2AssetRouter` deployed. Doing bridging to such chains will lead to funds being lost without recovery (since formally
the L1->L2 transaction won't fail as it is just a call to an empty address).

This is why it is crucial that on step (2) we revoke the ability for outdated chains to push new batches as those
might've been spawned using the `L1AssetRouter`.
