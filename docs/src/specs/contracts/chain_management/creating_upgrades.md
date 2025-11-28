# Creating upgrades

## Intro

Creating upgrades is an involved process for core zksync-devs. The process has multiple steps. 

## Steps

### 0. Create upgrade contracts

Some upgrades require custom L1 and L2 contracts. Normally the `DefaultUpgrade` contract can be used on L1, and no custom L2 contracts are needed.

### 1. Upgrade EcosystemUpgrade scripts

[These](https://github.com/matter-labs/era-contracts/blob/main/l1-contracts/deploy-scripts/upgrade/README.md) scripts generate the upgrade calldata, and the output of it is used in all later steps. Normally the DefaultEcosystemUpgrade can be used.

### 2. Run the local upgrade flow

This can be used for running the upgrade locally, including all changes. See [this](https://github.com/matter-labs/zksync-era/tree/main/infrastructure/local-gateway-upgrade-testing) on how to run it.

### 3. Upgrade protocol upgrade verification tool

Changes might be required for the [verification tool](https://github.com/matter-labs/protocol-upgrade-verification-tool).

### 4. Add transaction to txs simulator

Copy from upgrade verification tool to the [simulator](https://github.com/matter-labs/transaction-simulator/).

## Executing an upgrade

### Ecosystem upgrade

[From calls to upgrade](https://github.com/matter-labs/transaction-simulator/blob/main/docs/froms-calls-to-upgrade.md)

Use frontends:

- https://www.tally.xyz/gov/zksync
- https://verify.testnet.zknation.io/
- https://app.safe.global/welcome

### Chain upgrade

- use zkstack tool
