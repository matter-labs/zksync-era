# ZK Stack specs

![Logo](./zk-the-collective-action.jpeg)

## Introduction

This document serves as the specification of the ZK Stack protocol. The goal of the ZK Stack is to power the internet of
value by creating an interoperable ecosystem of hyperchains. Each of these hyperchains will be powered by the zkEVM,
using zero-knowledge proofs.

These specs will provide a high level overview of the zkEVM and a full specification of its more technical components,
such as the prover, compiler, and the VM itself.

We also specify the foundations of the hyperchain ecosystem including hyperbridging.

## Specification Contents

### zkEVM Overview

1. [L1 Contracts](./zkVM/high_level/1_l1_smart_contracts.md)
1. [VM internals](./zkVM/high_level/2_vm_internals.md)
1. [Bootloader](./zkVM/high_level/3_bootloader.md)
1. [L2 System Contracts](./zkVM/high_level/4_system_contracts.md)
1. [Precompiles](./zkVM/high_level/6_elliptic_curve_precompiles.md)
1. [Account Abstraction](./zkVM/high_level/5_account_abstraction.md)
1. [L1-> L2 messages, Deposits and Withdrawals](./zkVM/high_level/8_handling_L1→L2_ops.md)
1. [Fee model](./zkVM/high_level/10_fee_model/fee_model.md)
1. [Batches and L2 blocks](./zkVM/high_level/7_batches_L2_blocks.md)
1. [DA, Pubdata, Compression, Rollup, Validium and Porter](./zkVM/high_level/9_handling_pubdata_in_boojum/handling_pubdata_in_boojum.md)
1. [Prover](./zkVM/high_level/11_prover/zk_intuition.md)

### zkEVM and Prover full specification

- [VM](./zkVM/VM_and_prover/VM_section/zkSync_era_virtual_machine_primer.md)
- [Circuits](./zkVM/VM_and_prover/circuits_section/intro_to_zkSync’s_ZK.md)

### The Hyperchain

- [Shared Bridge](./the_hyperchain/1_shared_bridge.md)
- [Hyperbridging](./the_hyperchain/2_hyperbridges.md)

## Overview

The ZK Stack can be used to launch zero-knowledge rollups. Rollups are blockchains that rollup up their data and
execution to a base chain, in our case this is Ethereum. Rolling up data is relatively easy, the data required to
reconstruct the state of the rollup can be just sent to L1. Rolling up execution is very hard, th ZK Stack uses cutting
edge cryptography for this, this is the technology of zero-knowledge proofs. Zero-knowledge proofs use advanced
mathematics to show that the execution of the rollup was done correctly.

Using this method the rollup is as secure as L1, funds on the rollup can not be lost or stolen as long as the L1 is
secure, and they can always be withdrawn to L1.

The core actors that are needed to run a rollup are the sequencer, prover and the L1 contract.

A user submits their transaction to the sequencer. The job of the sequencer is to collect transactions and execute them
using the zkEVM, and to provide a soft confirmation to the user that their transaction was processed. If the user
chooses they can force the sequencer to include their transaction by submitting it via L1. After the sequencer executes
the block, it sends it over the prover, who creates a cryptographic proof of the blocks execution. This proof is then
sent to the L1 contract alongside the necessary data. On the L1 a
[smart contract](./zkVM/high_level/1_l1_smart_contracts.md) verifies that the proof is valid and all the data has been
submitted, and update the rollup's stored state on L1.

![Components](./L2_Components.png)

The zkSync zkEVM plays a fundamentally different role in the zkStack than the EVM does in Ethereum. The EVM is used to
execute code in Ethereum's state transition function. This STF needs a client to implement and run it.

Rollups have a different set of requirements, they need to produce a proof that some client executed the STF correctly.
This client is the [zkEVM](./zkVM/high_level/2_vm_internals.md), it is made to run the STF efficiently. The STF is the
[Bootloader](./zkVM/high_level/3_bootloader.md).

The smart contracts are native zkEVM bytecode, zkEVM can execute these easily. In the future the ZK Stack will also
support EVM bytecode by running an efficient interpreter inside the zkEVM.

The zkEVM has a lot of special features compared to the EVM that are needed for the rollup's STF, storage, gas metering,
precompiles etc. These functions are either built into the zkEVM, or there are special
[system contract](./zkVM/high_level/4_system_contracts.md) for them. The system contracts are deployed at predefined
addresses, they are called by the Bootloader, and they have special permissions compared to normal user contracts. These
are not to be confused with the [precompiles](./zkVM/high_level/6_elliptic_curve_precompiles.md), which are also
predeloyed contracts with special support from the zkEVM, but these contract do not have special permissions and are
called by the users and not the Bootloader.

Now that we understand the main components of the zkEVM lets look at executing a transaction. Transactions are normally
submitted by users directly to the sequencers. For the best possible UX the ZK Stack supports native
[account abstraction](./zkVM/high_level/5_account_abstraction.md). This means users can fully customize how they pay the
fees needed to execute their transactions.

Transactions can also be submitted via L1. This happens via the same process that allows L1<>L2 communication
[L1-> L2 messages, deposits and withdrawals](./zkVM/high_level/8_handling_L1→L2_ops.md). This method provides the rollup
with censorship resistance, and allows trustless bridges between the layers to be written by anybody.

All transactions need to pay fees. The requirements to run a rollup are different than the ones needed to run Ethereum,
so the ZK Stack has a different [fee model](./zkVM/high_level/10_fee_model/fee_model.md). The fee model is designed to
consider all the components that are needed to run the rollup: data and proof execution costs on L1, sequencer costs,
and prover costs.

Once the sequencer has enough transactions it collects them into [blocks](./zkVM/high_level/7_batches_L2_blocks.md),
similarly to Ethereum. To provide the best UX the protocol has small blocks with quick soft confirmations for the users.
Unlike Ethereum, the zkEVM does not just have blocks, but also batches. The batch is the unit that the prover processes,
and a batch contains multiple smaller blocks. Proofs can be aggregated, it is much cheaper for rollups to submit a
single proof for a whole batch instead of multiple proofs for each block.

There is another reason why big batches are advantageous, and this is how the ZK Stack handles
[data submission](./zkVM/high_level/9_handling_pubdata_in_boojum/handling_pubdata_in_boojum.md) to L1. Instead of
submitting the data of each transaction, we submit how the state of the blockchain changes, this change is called the
state diff. This approach allows the transactions that change the same storage slots to be very cheap, since these
transactions don't incur additional data costs. The state diff is also compressed when it is sent to L1.

Finally at the end of the process, we create the proofs and send them to L1. Our
[Boojum](./zkVM/high_level/11_prover/zk_intuition.md) proof system provides excellent performance, and can be run on
just 16Gb of GPU RAM. This will enable the proof generation to be truly decentralized.

Up to this point we have only talked about a single chain. These chains will be launched on L1 into a
[shared bridge](./the_hyperchain/1_shared_bridge.md). The shared bridge will create an ecosystem of chains, with shared
standards, upgrades, and free flow of assets. This free flow of assets will be enabled by
[hyperbridges](./the_hyperchain/2_hyperbridges.md). Hyperbridges are trustless and cheap bridges between hyperchains,
allowing cross-chain function calls.
