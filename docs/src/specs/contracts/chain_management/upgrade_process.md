<!--- WIP --->

# Upgrade process document


## Intro

This document assumes that you have understanding about [the structure](../settlement_contracts/zkchain_basics.md) on individual chains' L1 contracts.

Upgrading the ecosystem of ZKChains is a complicated process. ZKSync is a complex ecosystem with many chains and contracts and each upgrade is unique, but there are some steps that repeat for most upgrades. These are mostly how we interact with the CTM, the diamond facets, the L1→L2 upgrade, how we update the verification keys.

Where each upgrade consists of two parameters:

- Facet cuts - change of the internal implementation of the diamond proxy
- Diamond Initialization - delegate call to the specified address with specified data

The second parameter is very powerful and flexible enough to move majority of upgrade logic there.

## Preparation for the upgrade

The ZKsync ecosystem has [governance smart contracts](https://github.com/zksync-association/zk-governance) that govern the protocol. Only these contracts have the permission to set upgrades in the CTM. This is done via the `setNewVersionUpgrade` function. This sets the upgrade data, the new protocol version, and the deadline by which chains have to upgrade. Chains can upgrade themselves with the same data. After the deadline is over, each non-upgraded chain is frozen, they cannot post new proofs. Frozen chains can
unfreeze by updating.

## Upgrade structure

Upgrade information is composed in the form of a [DiamondCutData](https://github.com/matter-labs/era-contracts/blob/8222265420f362c853da7160769620d9fed7f834/l1-contracts/contracts/state-transition/libraries/Diamond.sol#L75) struct. During the upgrade, the chain's DiamondProxy will delegateCall the `initAddress` with the provided `initCalldata`, while the facets that the `DiamondProxy` will be changed according to the `facetCuts`. This scheme is very powerful and it allows to change anything in the contract. However, we typically have a very specific set of changes that we need to do. To facilitate these, two contracts have been created:

1. [BaseZkSyncUpgrade](https://github.com/matter-labs/era-contracts/blob/8222265420f362c853da7160769620d9fed7f834/l1-contracts/contracts/upgrades/BaseZkSyncUpgrade.sol) - Generic template with function that can be useful for upgrades
2. [DefaultUpgrade](https://github.com/matter-labs/era-contracts/blob/8222265420f362c853da7160769620d9fed7f834/l1-contracts/contracts/upgrades/DefaultUpgrade.sol) - Default implementation of the `BaseZkSyncUpgrade`, contract that is most often planned to be used as diamond initialization when doing upgrades.

> Note, that the Gateway upgrade (v26) was more complex than the usual ones and so a similar, but separate [process][TODO] was used for it. It also used its own custom implementation of the `BaseZkSyncUpgrade`: [GatewayUpgrade](https://github.com/matter-labs/era-contracts/blob/8222265420f362c853da7160769620d9fed7f834/l1-contracts/contracts/upgrades/GatewayUpgrade.sol).

### Protocol version

For tracking upgrade versions on different networks (private testnet, public testnet, mainnet) we use protocol version, which is basically just a number denoting the deployed version. The protocol version is different from Diamond Cut `proposalId`, since `protocolId` only shows how much upgrade proposal was proposed/executed, but nothing about the content of upgrades, while the protocol version is needed to understand what version is deployed.

In the [BaseZkSyncUpgrade](https://github.com/matter-labs/era-contracts/blob/8222265420f362c853da7160769620d9fed7f834/l1-contracts/contracts/upgrades/BaseZkSyncUpgrade.sol) & [DefaultUpgrade](https://github.com/matter-labs/era-contracts/blob/8222265420f362c853da7160769620d9fed7f834/l1-contracts/contracts/upgrades/DefaultUpgrade.sol) we allow to arbitrarily increase the proposal version while upgrading a system, but only increase it. We are doing that since we can skip some protocol versions if for example found a bug there (but it was deployed on another network already).

## Protocol upgrade transaction

During upgrade, we typically need not only update the L1 contracts, but also the L2 ones. This is achieved by creating an upgrade transactions. More details on how those are processed inside the system can be read [here][(../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md)].

## Whitelisting and executing upgrade

Note, that due to how powerful the upgrades are, if we allowed any [chain admin](../chain_management/admin_role.md) to inact any upgrade it wants, it could allow malicious chains to potentially break some of the ecosystem invariants. Because of that, any upgrade should be firstly whitelisted by the decentralized governance through calling the `setNewVersionUpgrade` function of the [ChainTypeManager](https://github.com/matter-labs/era-contracts/blob/8222265420f362c853da7160769620d9fed7f834/l1-contracts/contracts/state-transition/ChainTypeManager.sol).

In order to execute the upgrade, the chain admin would call the `upgradeChainFromVersion` function from the [Admin](https://github.com/matter-labs/era-contracts/blob/8222265420f362c853da7160769620d9fed7f834/l1-contracts/contracts/state-transition/chain-deps/facets/Admin.sol) facet.
