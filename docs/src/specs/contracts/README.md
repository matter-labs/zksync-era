<!--- WIP --->

# ZK Stack contracts specs

The order of the files here only roughly represents the order of reading. A lot of topics are intertwined, so it is recommended to read everything first to have a complete picture and then refer to specific documents for more details.

- [Overview](../contracts/overview.md)
- [Glossary](../contracts/glossary.md)
- [Chain Management](../contracts/chain_management/overview.md)
  - [Bridgehub](../contracts/chain_management/bridgehub.md)
  - [Chain type manager](../contracts/chain_management/chain_type_manager.md)
  - [Admin role](../contracts/chain_management/admin_role.md)
  - [Chain genesis](../contracts/chain_management/chain_genesis.md)
  - [Standard Upgrade process](../contracts/chain_management/upgrade_process.md)

![Reading order](./img/reading_order.png)

## Contracts repo structure

The repository contains the following sections:

- [gas-bound-caller][TODO] that contains `GasBoundCaller` utility contract implementation. You can read more about it in its README.
- [da-contracts][TODO] contracts that should be deployed on L1 only.
- [l1-contracts][TODO]. Despite the legacy name, it contains contracts that are deployed both on L1 and on L2. This folder encompasses bridging, ZK chain contracts, the contracts for chain admin, etc. The name is historical due to the fact that these contracts were usually deployed on L1 only. However with Gateway, settlement and bridging-related contracts will be deployed on both EVM and eraVM environment. Also, bridging has been unified between L1 and L2 in many places and so keeping everything in one project allows to avoid code duplication.
- [l2-contracts][TODO]. Contains contracts that are deployed only on L2.
- [system-contracts][TODO]. Contains system contracts or predeployed L2 contracts.

## For auditors: Invariants/tricky places to look out for

This section is for auditors of the codebase. It includes some of the important invariants that the system relies on and which if broken could have bad consequences.

- Assuming that the accepting CTM is correct & efficient, the L1→GW part of the L1→GW→L3 transaction never fails. It is assumed that the provided max amount for gas is always enough for any transaction that can realistically come from L1.
- GW → L1 migration never fails. If it is possible to get into a state where the migration is not possible to finish, then the chain is basically lost. There are some exceptions where for now it is the expected behavior. (check out the “Migration invariants & protocol upgradability” section)
- The general consistency of chains when migration between different settlement layers is done. Including the feasibility of emergency upgrades, etc. I.e. whether the whole system is thought-through.
- Preimage attacks in the L3→L1 tree, we apply special prefixes to ensure that the tree structure is fixed, i.e. all logs are 88 bytes long (this is for backwards compatibility reasons). For batch leaves and chain id leaves we use special prefixes.
- Data availability guarantees. Whether rollup users can always restore all their storage slots, etc. An example of a potential tricky issue can be found in “Security notes for Gateway-based rollups” [in this document][TODO].

The desired properties of the system are that funds can not be stolen from the L1 contracts, and that L2 constracts are executed securely.