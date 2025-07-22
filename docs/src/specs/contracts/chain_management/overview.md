# Overview

## Introduction

Ethereum's future is rollup-centric. This means breaking with the current paradigm of isolated EVM chains to
infrastructure that is focused on an ecosystem of interconnected zkEVMs, (which we name ZK Chains). This ecosystem will
be grounded on Ethereum, requiring the appropriate L1 smart contracts. Here we outline our ZK Stack approach for these
contracts, their interfaces, the needed changes to the existing architecture, as well as future features to be
implemented.

If you want to know more about ZK Chains, check this
[blog post](https://blog.matter-labs.io/introduction-to-hyperchains-fdb33414ead7), or go through
[our docs](https://docs.zksync.io/zk-stack/concepts/zk-chains).

### High-level design

We want to create a system where:

- ZK Chains should be launched permissionlessly within the ecosystem.
- Settlement should be made cheap by proof aggregation.
- Interop should enable unified liquidity for assets across the ecosystem.
- Multi-chain smart contracts need to be easy to develop, which means easy access to traditional bridges, and other
  supporting architecture.

ZK Chains have specific trust requirements - they need to satisfy certain common standards so that they can trust each
other. This means a single set of L1 smart contracts has to manage the proof verification for all ZK Chains, and if the
proof system is upgraded, all chains have to be upgraded together. New chains will be able to be launched
permissionlessly in the ecosystem according to this shared standard.

To allow unified liquidity each L1 asset (ETH, ERC20, NFTs) will have a single bridge contract on L1 for the whole
ecosystem. These shared bridges will allow users to deposit, withdraw and transfer from any ZK Chain in the ecosystem.
These shared bridges are also responsible for deploying and maintaining their counterparts on the ZK Chains. The
counterparts will be asset contracts extended with bridging functionality.

To enable the bridging functionality:

- On the L1 we will add a Bridgehub contract which connects asset bridges to all the ZK Chains.
- On the ZK Chain side we will add special system contracts that enable these features.

We want to make the ecosystem as modular as possible, giving developers the ability to modify the architecture as
needed; consensus mechanism, staking, and DA requirements.

We also want the system to be forward-compatible, alternative Chain Type Manager (CTM) contracts. Those future features have
to be considered in this initial design, so it can evolve to support them (meaning, chains being launched now will still
be able to leverage them when available).

---

## Architecture

### General Architecture

![Contracts](./img/ecosystem_architecture.png)

### Components

#### Bridgehub

- This is the main registry contract. This is where ZK Chains can register, starting in a permissioned manner, but with the goal to be
  permissionless in the future. This is where their `chainID` is determined. Chains on Gateway will also register here.
  This `Registry` is also where Chain Type Manager contracts should register. Each chain has to specify its desired CTM
  when registering (Initially, only one will be available).

  ```
  function newChain(
          uint256 _chainId,
          address _chainTypeManager
      ) external returns (uint256 chainId);

  function newChainTypeManager(address _chainTypeManager) external;
  ```

#### Chain Type Manager

- `ChainTypeManager` A chain type manager manages proof verification and standard rollup DA for multiple chains. It also implements the
  following functionalities:
  - `ChainTypeRegistry` The chain type is shared for multiple chains, so initialization and upgrades have to be the same for all
    chains. Registration is not permissionless but happens based on the registrations in the bridgehub’s `Registry`. At
    registration a `DiamondProxy` is deployed and initialized with the appropriate `Facets` for each ZK Chain.
  - `Facets` and `Verifier` are shared across chains that relies on the same chain type: `Base`, `Executor` , `Getters`, `Admin`
    , `Mailbox.`The `Verifier` is the contract that actually verifies the proof, and is called by the `Executor`.
  - Upgrade Mechanism The system requires all chains to be up-to-date with the latest implementation, so whenever an
    update is needed, we have to “force” each chain to update, but due to decentralization, we have to give each chain a
    time frame. This is done in the update mechanism contract, this is where the bootloader and system contracts are
    published, and the `ProposedUpgrade` is stored. Then each chain can call this upgrade for themselves as needed.
    After the deadline is over, the not-updated chains are frozen, that is, cannot post new proofs. Frozen chains can
    unfreeze by updating their proof system.
- Each chain has a `DiamondProxy`.
  - The [Diamond Proxy](https://eips.ethereum.org/EIPS/eip-2535) is the proxy pattern that is used for the chain
    contracts. A diamond proxy points to multiple implementation contracts called facets. Each selector is saved in the
    proxy, and the correct facet is selected and called.
  - In the future the DiamondProxy can be configured by picking alternative facets e.g. Validiums will have their own
    `Executor`

#### Chain specific contracts

- A chain might implement its own specific consensus mechanism. This needs its own contracts. Only this contract will be
  able to submit proofs to the State Transition contract.
- DA contracts.
- Currently, the `ValidatorTimelock` is an example of such a contract.

### Components interactions

In this section, we will present some diagrams showing the interaction of different components.

#### New Chain

A chain registers in the Bridgehub, this is where the chain ID is determined. Read more about it [here](./chain_genesis.md). The chain’s governor specifies the State
Transition that they plan to use. In the first version only a single State Transition contract will be available for
use, our with Boojum proof verification.

At initialization we prepare the `DiamondInit` contract. We store the genesis batch hash in the chain contract, all
chains start out with the same state. A diamond proxy is deployed and initialised with this initial value, along with
predefined facets which are made available by the chain contract. These facets contain the proof verification and other
features required to process proofs. The chain ID is set in the VM in a special system transaction sent from L1.

---

### Common Standards and Upgrades

In this initial phase, ZK Chains have to follow some common standards, so that they can trust each other. Read more about it [here](./upgrade_process.md). This means all
chains start out with the same empty state, they have the same VM implementations and proof systems, asset contracts can
trust each on different chains, and the chains are upgraded together. We elaborate on the shared upgrade mechanism here.

#### Upgrade mechanism

Currently, there are three types of upgrades for zkEVM. Normal upgrades (used for new features) are initiated by the
Governor (a multisig) and are public for a certain timeframe before they can be applied. Shadow upgrades are similar to
normal upgrades, but the data is not known at the moment the upgrade is proposed, but only when executed (they can be
executed with the delay, or instantly if approved by the security council). Instant upgrades (used for security issues),
on the other hand happen quickly and need to be approved by the Security Council in addition to the Governor. For ZK
Chains the difference is that upgrades now happen on multiple chains. This is only a problem for shadow upgrades - in
this case, the chains have to tightly coordinate to make all the upgrades happen in a short time frame, as the content
of the upgrade becomes public once the first chain is upgraded. The actual upgrade process is as follows:

1. Prepare Upgrade for all chains:
   - The new facets and upgrade contracts have to be deployed,
   - The upgrade’ calldata (diamondCut, initCalldata with ProposedUpgrade) is hashed on L1 and the hash is saved.
2. Upgrade specific chain
   - The upgrade has to be called on the specific chain. The upgrade calldata is passed in as calldata and verified. The
     protocol version is updated.
   - Ideally, the upgrade will be very similar for all chains. If it is not, a smart contract can calculate the
     differences. If this is also not possible, we have to set the `diamondCut` for each chain by hand.
3. Freeze not upgraded chains
   - After a certain time the chains that are not upgraded are frozen.