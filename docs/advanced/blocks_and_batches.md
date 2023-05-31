# Blocks & Batches - How we package transactions

In this article, we will explore the processing of transactions, how we group them into blocks, what it means to "seal"
a block, and why it is important to have rollbacks in our virtual machine (VM).

At the basic level, we have individual transactions. However, to execute them more efficiently, we group them together
into blocks & batches

## L1 Batch vs L2 Block (a.k.a MiniBlock) vs Transaction

To help visualize the concept, here are two images:

![Block layout][block_layout]

You can refer to the Block layout image to see how the blocks are organized. It provides a graphical representation of
how transactions are arranged within the blocks and the arrangement of L2 blocks within L1 "batches."

![Explorer example][explorer_example]

### L2 blocks (aka Miniblocks)

Currently, the L2 blocks do not have a major role in the system, until we transition to a decentralized sequencer. We
introduced them mainly as a "compatibility feature" to accommodate various tools, such as Metamask, which expect a block
that changes frequently. This allows these tools to provide feedback to users, confirming that their transaction has
been added.

As of now, an L2 block is created every 2 seconds (controlled by StateKeeper's config `miniblock_commit_deadline_ms`),
and it includes all the transactions received during that time period. This periodic creation of L2 blocks ensures that
transactions are processed and included in the blocks regularly.

### L1 batches

L1 batches play a crucial role because they serve as the fundamental unit for generating proofs. From the perspective of
the virtual machine (VM), each L1 batch represents the execution of a single program, specifically the Bootloader. The
Bootloader internally processes all the transactions belonging to that particular batch. Therefore, the L1 batch serves
as the container for executing the program and handling the transactions within it.

#### So how large can L1 batch be

Most blockchains use factors like time and gas usage to determine when a block should be closed or sealed. However, our
case is a bit more complex because we also need to consider prover capacity and limits related to publishing to L1.

The decision of when to seal the block is handled by the code in the [conditional_sealer][conditional_sealer] module. It
maintains a list of `SealCriterion` and at the time of writing this article, [we have 9 reasons to seal the
block][reasons_for_sealing], which include:

- Transaction slots limit (currently set to 750 transactions in `StateKeeper`'s config - `transaction_slots`).
- Gas limit (currently set to `MAX_L2_TX_GAS_LIMIT` = 80M).
- Published data limit (as each L1 batch must publish information about the changed slots to L1, so all the changes must
  fit within the L1 transaction limit, currently set to `MAX_PUBDATA_PER_L1_BATCH`= 120k).
- zkEVM Geometry limits - For certain operations like merklelization, there is a maximum number of circuits that can be
  included in a single L1 batch. If this limit is exceeded, we wouldn't be able to generate the proof.

We also have a `TimeoutCriterion` - but it is not enabled.

However, these sealing criteria pose a significant challenge because it is difficult to predict in advance whether
adding a given transaction to the current batch will exceed the limits or not. This unpredictability adds complexity to
the process of determining when to seal the block.

#### What if a transaction doesn't fit

To handle situations where a transaction exceeds the limits of the currently active L1 batch, we employ a "try and
rollback" approach. This means that we attempt to add the transaction to the active L1 batch, and if we receive a
`ExcludeAndSeal` response indicating that it doesn't fit, we roll back the virtual machine (VM) to the state before the
transaction was attempted.

Implementing this approach introduces a significant amount of complexity in the `oracles` (also known as interfaces) of
the VM. These oracles need to support snapshotting and rolling back operations to ensure consistency when handling
transactions that don't fit.

In a separate article, we will delve into more details about how these oracles and the VM work, providing a
comprehensive understanding of their functionality and interactions.

[block_layout]:
  https://user-images.githubusercontent.com/128217157/236494232-aeed380c-78f6-4fda-ab2a-8de26c1089ff.png
  'block layout'
[explorer_example]:
  https://user-images.githubusercontent.com/128217157/236500717-165470ad-30b8-4ad6-97ed-fc29c8eb1fe0.png
  'explorer example'
[conditional_sealer]:
  https://github.com/matter-labs/zksync-2-dev/blob/1ef7fd03c1cbd175dc9be1309ec7698d91d90571/core/bin/zksync_core/src/state_keeper/seal_criteria/conditional_sealer.rs#L21
  'Conditional Sealer'
[reasons_for_sealing]:
  https://github.com/matter-labs/zksync-2-dev/blob/1ef7fd03c1cbd175dc9be1309ec7698d91d90571/core/bin/zksync_core/src/state_keeper/seal_criteria/conditional_sealer.rs#L119
  'Reasons for Sealing'
