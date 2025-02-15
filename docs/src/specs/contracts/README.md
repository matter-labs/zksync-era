# ZK Stack contracts specs

The order of the files here only roughly represents the order of reading. A lot of topics are intertwined, so it is recommended to read everything first to have a complete picture and then refer to specific documents for more details.

- [Overview](./overview.md)
- [Glossary](./glossary.md)
- [Chain Management](./chain_management/overview.md)
  - [Bridgehub](./chain_management/bridgehub.md)
  - [Chain type manager](./chain_management/chain_type_manager.md)
  - [Admin role](./chain_management/admin_role.md)
  - [Chain genesis](./chain_management/chain_genesis.md)
  - [Standard Upgrade process](./chain_management/upgrade_process.md)
- [Bridging](./bridging/overview.md)
  - [Interop](./bridging/interop/overview.md)
    - [Overview of the  functionality](./bridging/interop/interop_center/overview.md)
      - [Interop Center](./bridging/interop/interop_center/interop_center.md)
      - [Interop Messages](./bridging/interop/interop_center/interop_messages.md)
      - [Bundles and Calls](./bridging/interop/interop_center/bundles_calls.md)
      - [Interop Trigger](./bridging/interop/interop_center/interop_trigger.md)
    - Examples
      - [Cross chain message](./bridging/interop/examples/cross_chain_message.md)
      - [Cross chain paymaster](./bridging/interop/examples/cross_chain_paymaster.md)
      - [Cross chain swap](./bridging/interop/examples/cross_chain_swap.md)
      - [Interop request direct](./bridging/interop/examples/interop_request_direct.md)
      - [Interop request two bridges](./bridging/interop/examples/interop_request_two_bridges.md)
      - [Interop CTM deployment](./bridging/interop/examples/interop_ctm_deployment.md)
    - [Interop handler](./bridging/interop/interop_handler.md)
    - [Message root](./bridging/interop/message_root.md)
    - [Forms of finality](./bridging/interop/forms_of_finality.md)
  - [Asset Router](./bridging/asset_router_and_ntv/asset_router.md)
  - [Native token vault](./bridging/asset_router_and_ntv/native_token_vault.md)
  - [Asset Tracker](./bridging/firewall/firewall.md)
- [Settlment Contracts](./settlement_contracts/zkchain_basics.md)
  - L1->L2 transaction handling
    - [Processing of L1->L2 transactions](./settlement_contracts/priority_queue/processing_of_l1-l2_txs.md)
    - [Priority queue](./settlement_contracts/priority_queue/priority-queue.md)
  - Data availability
    - [Pubdata](./settlement_contracts/data_availability/pubdata.md)
    - [Compression](./settlement_contracts/data_availability/compression.md)
    - [Reconstruction](./settlement_contracts/data_availability/reconstruction.md)
    - [Validium and zkPorter](./settlement_contracts/data_availability/validium_zk_porter.md)
    - [Custom DA support](./settlement_contracts/data_availability/custom_da.md)
    - [Rollup DA support](./settlement_contracts/data_availability/rollup_da.md)
    - [Standard pubdata format](./settlement_contracts/data_availability/standard_pubdata_format.md)
    - [State diff compression v1 spec](./settlement_contracts/data_availability/state_diff_compression_v1_spec.md)
- [Gateway](./gateway/overview.md)
  - [Chain migration](./gateway/chain_migration.md)
  - [L1->L3 messaging via gateway](./gateway/messaging_via_gateway.md)
  - [L3->L1 messaging via gateway](./gateway/nested_l3_l1_messaging.md)
  - [Gateway protocol versioning](./gateway/gateway_protocol_upgrades.md)
  - [DA handling on Gateway](./gateway/gateway_da.md)
- Consensus
  - [Consensus Registry](./consensus/consensus-registry.md)
- L2 System Contracts
  - [System contracts bootloader description](./l2_system_contracts/system_contracts_bootloader_description.md)
  - [Batches and blocks on ZKsync](./l2_system_contracts/batches_and_blocks_on_zksync.md)
  - [Elliptic curve precompiles](./l2_system_contracts/elliptic_curve_precompiles.md)
  - [ZKsync fee model](./l2_system_contracts/zksync_fee_model.md)
<!-- - Upgrade history
  - [Gateway upgrade diff](./upgrade_history/gateway_upgrade/gateway_diff_review.md)
  - [Gateway upgrade process](./upgrade_history/gateway_upgrade/upgrade_process.md) -->

![Reading order](./img/reading_order.png)

## Repo structure

The repository contains the following sections:

- [gas-bound-caller](../gas-bound-caller) that contains `GasBoundCaller` utility contract implementation. You can read more about it in its README.
- [da-contracts](../da-contracts/). There are implementations for [DA validation](./settlement_contracts/data_availability/custom_da.md) contracts that should be deployed on L1 only.
- [l1-contracts](../l1-contracts/). Despite the legacy name, it contains contracts that are deployed both on L1 and on L2. This folder encompasses bridging, ZK chain contracts, the contracts for chain admin, etc. The name is historical due to the fact that these contracts were usually deployed on L1 only. However with Gateway, settlement and bridging-related contracts will be deployed on both EVM and eraVM environment. Also, bridging has been unified between L1 and L2 in many places and so keeping everything in one project allows to avoid code duplication.
- [l2-contracts](../l2-contracts/). Contains contracts that are deployed only on L2.
- [system-contracts](../system-contracts/). Contains system contracts or predeployed L2 contracts.

## For auditors: Invariants/tricky places to look out for

This section is for auditors of the codebase. It includes some of the important invariants that the system relies on and which if broken could have bad consequences.

- Assuming that the accepting CTM is correct & efficient, the L1→GW part of the L1→GW→L3 transaction never fails. It is assumed that the provided max amount for gas is always enough for any transaction that can realistically come from L1.
- GW → L1 migration never fails. If it is possible to get into a state where the migration is not possible to finish, then the chain is basically lost. There are some exceptions where for now it is the expected behavior. (check out the “Migration invariants & protocol upgradability” section)
- The general consistency of chains when migration between different settlement layers is done. Including the feasibility of emergency upgrades, etc. I.e. whether the whole system is thought-through.
- Preimage attacks in the L3→L1 tree, we apply special prefixes to ensure that the tree structure is fixed, i.e. all logs are 88 bytes long (this is for backwards compatibility reasons). For batch leaves and chain id leaves we use special prefixes.
- Data availability guarantees. Whether rollup users can always restore all their storage slots, etc. An example of a potential tricky issue can be found in “Security notes for Gateway-based rollups” [in this document](./gateway/gateway_da.md).

The desired properties of the system are that funds can not be stolen from the L1 contracts, and that L2 constracts are executed securely.