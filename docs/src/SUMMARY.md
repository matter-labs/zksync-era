<!-- markdownlint-disable-file MD025 -->

# Summary

[Introduction](README.md)

# Guides

- [Basic](guides/README.md)

  - [Setup Dev](guides/setup-dev.md)
  - [Development](guides/development.md)
  - [Launch](guides/launch.md)
  - [Architecture](guides/architecture.md)
  - [Build Docker](guides/build-docker.md)
  - [Repositories](guides/repositories.md)

- [Advanced](guides/advanced/README.md)
  - [Local initialization](guides/advanced/01_initialization.md)
  - [Deposits](guides/advanced/02_deposits.md)
  - [Withdrawals](guides/advanced/03_withdrawals.md)
  - [Contracts](guides/advanced/04_contracts.md)
  - [Calls](guides/advanced/05_how_call_works.md)
  - [Transactions](guides/advanced/06_how_transaction_works.md)
  - [Fee Model](guides/advanced/07_fee_model.md)
  - [L2 Messaging](guides/advanced/08_how_l2_messaging_works.md)
  - [Pubdata](guides/advanced/09_pubdata.md)
  - [Pubdata with Blobs](guides/advanced/10_pubdata_with_blobs.md)
  - [Bytecode compression](guides/advanced/11_compression.md)
  - [EraVM intro](guides/advanced/12_alternative_vm_intro.md)
  - [ZK Intuition](guides/advanced/13_zk_intuition.md)
  - [ZK Deeper Dive](guides/advanced/14_zk_deeper_overview.md)
  - [Prover Keys](guides/advanced/15_prover_keys.md)
  - [Decentralization](guides/advanced/16_decentralization.md)
  - [L1 Batch Reversion](guides/advanced/17_batch_reverter.md)
  - [Advanced Debugging](guides/advanced/90_advanced_debugging.md)
  - [Docker and CI](guides/advanced/91_docker_and_ci.md)

# External Node

- [External node](guides/external-node/01_intro.md)
  - [Quick Start](guides/external-node/00_quick_start.md)
  - [Configuration](guides/external-node/02_configuration.md)
  - [Running](guides/external-node/03_running.md)
  - [Observability](guides/external-node/04_observability.md)
  - [Troubleshooting](guides/external-node/05_troubleshooting.md)
  - [Components](guides/external-node/06_components.md)
  - [Snapshots Recovery](guides/external-node/07_snapshots_recovery.md)
  - [Pruning](guides/external-node/08_pruning.md)
  - [Treeless Mode](guides/external-node/09_treeless_mode.md)
  - [Decentralization](guides/external-node/10_decentralization.md)
  - [Setup for other chains](guides/external-node/11_setup_for_other_chains.md)

# Specs

- [Introduction](specs/introduction.md)
  - [Overview](specs/overview.md)
  - [Blocks and Batches](specs/blocks_batches.md)
  - [L1 Smart Contracts](specs/l1_smart_contracts.md)
- [Contracts](specs/contracts/README.md)
  - [Overview](specs/contracts/overview.md)
  - [Glossary](specs/contracts/glossary.md)
  - [Chain Management](specs/contracts/chain_management/overview.md)
    - [Bridgehub](specs/contracts/chain_management/bridgehub.md)
    - [Chain type manager](specs/contracts/chain_management/chain_type_manager.md)
    - [Admin role](specs/contracts/chain_management/admin_role.md)
    - [Chain genesis](specs/contracts/chain_management/chain_genesis.md)
    - [Standard Upgrade process](specs/contracts/chain_management/upgrade_process.md)
- [Prover](specs/prover/overview.md)
  - [Getting Started](specs/prover/getting_started.md)
  - [ZK Terminology](specs/prover/zk_terminology.md)
  - [Function Check if Satisfied](specs/prover/boojum_function_check_if_satisfied.md)
  - [Gadgets](specs/prover/boojum_gadgets.md)
  - [Circuit Testing](specs/prover/circuit_testing.md)
  - [Circuits Overview](specs/prover/circuits/overview.md)
- [Era VM](specs/era_vm_specification/README.md)
  - [VM primer](specs/era_vm_specification/zkSync_era_virtual_machine_primer.md)

# Announcements

- [Announcements](announcements/README.md)
  - [Attester Committee](announcements/attester_commitee.md)
