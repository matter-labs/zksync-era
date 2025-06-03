# Blocks & Batches - How we package transactions

In this article, we will explore the processing of transactions, how we group them into blocks, what it means to "seal"
a block, and why it is important to have rollbacks in our virtual machine (VM).

At the basic level, we have individual transactions. However, to execute them more efficiently, we group them together
into blocks & batches.

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
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/state_keeper/seal_criteria/conditional_sealer.rs#20
  'Conditional Sealer'
[reasons_for_sealing]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/state_keeper/seal_criteria/mod.rs#L106
  'Reasons for Sealing'

## Deeper dive

### Glossary

- Batch - a set of transactions that the bootloader processes (`commitBatches`, `proveBatches`,
  and `executeBatches` work with it). A batch consists of multiple transactions.
- L2 block - a non-intersecting sub-set of consecutively executed transactions. This is the kind of block you see in the
  API. This is the one that will _eventually_ be used for `block.number`/`block.timestamp`/etc. This will happen
  _eventually_, since at the time of this writing the virtual block migration is being
  [run](#migration--virtual-blocks-logic).
- Virtual block — blocks the data of which will be returned in the contract execution environment during the migration.
  They are called “virtual”, since they have no trace in our API, i.e. it is not possible to query information about
  them in any way.

### Motivation

Before the recent upgrade, `block.number`, `block.timestamp`, as well as `blockhash` in Solidity, returned information
about _batches_, i.e. large blocks that are proven on L1 and which consist of many small L2 blocks. At the same time,
API returns `block.number` and `block.timestamp` as for L2 blocks.

L2 blocks were created for fast soft confirmation on wallets and block explorer. For example, MetaMask shows
transactions as confirmed only after the block in which transaction execution was mined. So if the user needs to wait
for the batch confirmation it would take at least minutes (for soft confirmation) and hours for full confirmation which
is very bad UX. But API could return soft confirmation much earlier through L2 blocks.

There was a huge outcry in the community for us to return the information for L2 blocks in `block.number`,
`block.timestamp`, as well as `blockhash`, because of discrepancy of runtime execution and returned data by API.

However, there were over 15mln L2 blocks, while less than 200k batches, meaning that if we simply “switched” from
returning L1 batches’ info to L2 block’s info, some contracts (especially those that use `block.number` for measuring
time intervals instead of `block.timestamp`) would break. For that, we decided to have an accelerated migration process,
i.e. the `block.number` will grow faster and faster, until it becomes roughly 8x times the L2 block production speed,
allowing it to gradually reach the L2 block number, after which the information on the L2 `block.number` will be
returned. The blocks the info of which will be returned during this process are called “virtual blocks”. Their
information will never be available in any of our APIs, which should not be a major breaking change, since our API
already mostly works with L2 blocks, while L1 batches’s information is returned in the runtime.

### Adapting for Solidity

In order to get the returned value for `block.number`, `block.timestamp`, `blockhash` our compiler used the following
functions:

- `getBlockNumber`
- `getBlockTimestamp`
- `getBlockHashEVM`

During the migration process, these will return the values of the virtual blocks. After the migration is complete, they
will return values for L2 blocks.

### Migration status

At the time of this writing, the migration has been complete on testnet, i.e. there we already have only the L2 block
information returned. However, the [migration](https://github.com/zkSync-Community-Hub/zksync-developers/discussions/87)
on mainnet is still ongoing and most likely will end on late October / early November.

## Blocks’ processing and consistency checks

Our `SystemContext` contract allows to get information about batches and L2 blocks. Some of the information is hard to
calculate onchain. For instance, time. The timing information (for both batches and L2 blocks) are provided by the
operator. In order to check that the operator provided some realistic values, certain checks are done on L1. Generally
though, we try to check as much as we can on L2.

## Initializing L1 batch

At the start of the batch, the operator
[provides](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L3636)
the timestamp of the batch, its number and the hash of the previous batch.. The root hash of the Merkle tree serves as
the root hash of the batch.

The SystemContext can immediately check whether the provided number is the correct batch number. It also immediately
sends the previous batch hash to L1, where it will be checked during the commit operation. Also, some general
consistency checks are performed. This logic can be found
[here](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/SystemContext.sol#L416).

## L2 blocks processing and consistency checks

### `setL2Block`

Before each transaction, we call `setL2Block`
[method](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L2605).
There we will provide some data about the L2 block that the transaction belongs to:

- `_l2BlockNumber` The number of the new L2 block.
- `_l2BlockTimestamp` The timestamp of the new L2 block.
- `_expectedPrevL2BlockHash` The expected hash of the previous L2 block.
- `_isFirstInBatch` Whether this method is called for the first time in the batch.
- `_maxVirtualBlocksToCreate` The maximum number of virtual block to create with this L2 block.

If two transactions belong to the same L2 block, only the first one may have non-zero `_maxVirtualBlocksToCreate`. The
rest of the data must be same.

The `setL2Block`
[performs](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/SystemContext.sol#L312)
a lot of similar consistency checks to the ones for the L1 batch.

### L2 blockhash calculation and storage

Unlike L1 batch’s hash, the L2 blocks’ hashes can be checked on L2.

The hash of an L2 block is
`keccak256(abi.encode(_blockNumber, _blockTimestamp, _prevL2BlockHash, _blockTxsRollingHash))`. Where
`_blockTxsRollingHash` is defined in the following way:

`_blockTxsRollingHash = 0` for an empty block.

`_blockTxsRollingHash = keccak(0, tx1_hash)` for a block with one tx.

`_blockTxsRollingHash = keccak(keccak(0, tx1_hash), tx2_hash)` for a block with two txs, etc.

To add a transaction hash to the current miniblock we use the `appendTransactionToCurrentL2Block`
[function](https://github.com/matter-labs/era-contracts/blob/main/system-contracts/contracts/SystemContext.sol#L427).

Since ZKsync is a state-diff based rollup, there is no way to deduce the hashes of the L2 blocks based on the
transactions’ in the batch (because there is no access to the transaction’s hashes). At the same time, in order to
server `blockhash` method, the VM requires the knowledge of some of the previous L2 block hashes. In order to save up on
pubdata (by making sure that the same storage slots are reused, i.e. we only have repeated writes) we
[store](https://github.com/matter-labs/era-contracts/blob/main/system-contracts/contracts/SystemContext.sol#L74)
only the last 257 block hashes. You can read more on what are the repeated writes and how the pubdata is processed
[here](./contracts/settlement_contracts/data_availability/pubdata.md).

We store only the last 257 blocks, since the EVM requires only 256 previous ones and we use 257 as a safe margin.

### Legacy blockhash

When initializing L2 blocks that do not have their hashes stored on L2 (basically these are blocks before the migration
upgrade), we use the following formula for their hash:

`keccak256(abi.encodePacked(uint32(_blockNumber)))`

### Timing invariants

While the timestamp of each L2 block is provided by the operator, there are some timing invariants that the system
preserves:

- For each L2 block its timestamp should be > the timestamp of the previous L2 block
- For each L2 block its timestamp should be ≥ timestamp of the batch it belongs to
- Each batch must start with a new L2 block (i.e. an L2 block can not span across batches).
- The timestamp of a batch must be ≥ the timestamp of the latest L2 block which belonged to the previous batch.
- The timestamp of the last miniblock in batch can not go too far into the future. This is enforced by publishing an
  L2→L1 log, with the timestamp which is then checked on L1.

## Fictive L2 block & finalizing the batch

At the end of the batch, the bootloader calls the `setL2Block`
[one more time](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L3812)
to allow the operator to create a new empty block. This is done purely for some of the technical reasons inside the
node, where each batch ends with an empty L2 block.

We do not enforce that the last block is empty explicitly as it complicates the development process and testing, but in
practice, it is, and either way, it should be secure.

Also, at the end of the batch we send the timestamps of the batch as well as the timestamp of the last miniblock in
order to check on L1 that both of these are realistic. Checking any other L2 block’s timestamp is not required since all
of them are enforced to be between those two.

## Migration & virtual blocks’ logic

As already explained above, for a smoother upgrade for the ecosystem, there is a migration being performed during which
instead of returning either batch information or L2 block information, we will return the virtual block information
until they catch up with the L2 block’s number.

### Production of the virtual blocks

- In each batch, there should be at least one virtual block created.
- Whenever a new L2 block is created, the operator can select how many virtual blocks it wants to create. This can be
  any number, however, if the number of the virtual block exceeds the L2 block number, the migration is considered
  complete and we switch to the mode where the L2 block information will be returned.

## Additional note on blockhashes

Note, that if we used some complex formula for virtual blocks’ hashes (like we do for L2 blocks), we would have to put
all of these into storage for the data availability. Even if we used the same storage trick that we used for the L2
blocks, where we store only the last 257’s block’s hashes under the current load/migration plans it would be expected
that we have roughly ~250 virtual blocks per batch, practically meaning that we will publish all of these anyway. This
would be too expensive. That is why we have to use a simple formula of `keccak(uint256(number))` for now. Note, that
they do not collide with the legacy miniblock hash, since legacy miniblock hashes are calculated as
`keccak(uint32(number))`.

Also, we need to keep the consistency of previous blockhashes, i.e. if `blockhash(X)` returns a non-zero value, it
should be consistent among the future blocks. For instance, let’s say that the hash of batch `1000` is `1`,
i.e. `blockhash(1000) = 1`. Then, when we migrate to virtual blocks, we need to ensure that `blockhash(1000)` will
return either 0 (if and only if the block is more than 256 blocks old) or `1`. Because of that for `blockhash` we will
have the following complex
[logic](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/SystemContext.sol#L103):

- For blocks that were created before the virtual block upgrade, use the batch hashes
- For blocks that were created during the virtual block upgrade, use `keccak(uint256(number))`.
- For blocks that were created after the virtual blocks have caught up with the L2 blocks, use L2 block hashes.
