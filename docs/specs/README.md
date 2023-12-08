# ZK Stack specs

![Logo](./zk-the-collective-action.jpeg)

## Specification Contents

- zkVM Overview
  <head> 
  <style> ol {list-style-type: lower-arabic;} 
  </style>
  </head>

  <ol>
  <li>
    <a href="./zkVM/high_level/1_l1_smart_contracts.md "> L1 Contracts</a> </li>
    <li> [VM internals](./zkVM/high_level/2_vm_internals.md)</li>
    <li> [Bootloader](./zkVM/high_level/3_bootloader.md)</li>
    <li> [L2 System Contracts](./zkVM/high_level/4_system_contracts.md)</li>
    <li> [Account Abstraction](./zkVM/high_level/5_account_abstraction.md)</li>
    <li> [Precompiles](./zkVM/high_level/6_elliptic_curve_precompiles.md)</li>
    <li> [Batches and L2 blocks](./zkVM/high_level/7_batches_L2_blocks.md)</li>
    <li> [L1-> L2 messages, Deposits and Withdrawals](./zkVM/high_level/8_handling_L1→L2_ops.md)</li>
    <li> DA, Pubdata, Compression, Rollup, Validium and Porter](./zkVM/high_level/9_handling_pubdata_in_boojum/bytecode_compression.md)</li>
    <li> [Fee model](./zkVM/high_level/10_fee_model/fee_model.md)</li>
    <li> [Prover](./zkVM/high_level/11_prover/zk_intuition.md)</li>
  </ol>

- zkVM and Prover full specification
  - [VM](./zkVM/VM_and_prover/VM_section/zkSync_era_virtual_machine_primer.md)
  - [Circuits](./zkVM/VM_and_prover/circuits_section/intro_to_zkSync’s_ZK.md)
- [Decentralisation](./zkVM/Decentralisation/network_design_for_zkSync_BFT.md)
- The Hyperchain
  - [Shared Bridge](./the_hyperchain/1_shared_bridge.md)
  - [Hyperbridging](./the_hyperchain/2_hyperbridges.md)

## Overview

This document is a specification of the ZK Stack protocol.

The main part focuses on the zkVM, which serves to prove the state transition function of all hyperchains. This provides
a high level overview of the zkVM, and a full specification of its more technical components (such as the prover,
compiler, and a full specification of the VM itself).

We also specify the decentralised consensus mechanism, and the hyperchain ecosystem including hyperbridging.
