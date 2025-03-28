# Batches & L2 blocks on ZKsync

../README.md)

## Glossary

- Batch - a set of transactions that the bootloader processes (`commitBatches`, `proveBatches`, and `executeBatches` work with it). A batch consists of multiple transactions.
- L2 blocks - non-intersecting sub-sets of consecutively executed transactions in a batch. This is the kind of block you see in the API. This is the one that is used for `block.number`/`block.timestamp`/etc.

> Note that sometimes in code you can see notion of "virtual blocks". In the past, we returned batch information for `block.number`/`block.timestamp`. However due to DevEx issues we decided to move to returned these values for L2 blocks. Virtual blocks were used during migration, but are not used anymore. You consider that there is one virtual block per one L2 block and it has exactly the same properties.

## Motivation

L2 blocks were created for fast soft confirmation in wallets and block explorer. For example, MetaMask shows transactions as confirmed only after the block in which transaction execution was mined. So if the user needs to wait for the batch confirmation it would take at least a few minutes (for soft confirmation) and hours for full confirmation which is very bad UX. But API could return soft confirmation much earlier through L2 blocks.

## Adapting for Solidity

In order to get the returned value for `block.number`, `block.timestamp`, `blockhash` our compiler used the following functions:

- `getBlockNumber`
- `getBlockTimestamp`
- `getBlockHashEVM`

These return values for L2 blocks.

## Blocks’ processing and consistency checks

Our `SystemContext` contract allows to get information about batches and L2 blocks. Some of the information is hard to calculate onchain. For instance, time. The timing information (for both batches and L2 blocks) are provided by the operator. In order to check that the operator provided some realistic values, certain checks are done on L1. Generally though, we try to check as much as we can on L2.

### Initializing L1 batch

At the start of the batch, the operator [provides](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L3935) the timestamp of the batch, its number and the hash of the previous batch. The root hash of the Merkle tree serves as the root hash of the batch.

The SystemContext can immediately check whether the provided number is the correct batch number. It also immediately sends the previous batch hash to L1, where it will be checked during the commit operation. Also, some general consistency checks are performed. This logic can be found [here](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/SystemContext.sol#L469).

### L2 blocks processing and consistency checks

#### `setL2Block`

Before each transaction, we call `setL2Block` [method](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L2884). There we will provide some data about the L2 block that the transaction belongs to:

- `_l2BlockNumber` The number of the new L2 block.
- `_l2BlockTimestamp` The timestamp of the new L2 block.
- `_expectedPrevL2BlockHash` The expected hash of the previous L2 block.
- `_isFirstInBatch` Whether this method is called for the first time in the batch.
- `_maxVirtualBlocksToCreate` The maximum number of virtual block to create with this L2 block. This is a legacy field that is always equal either to 0 or 1.

If two transactions belong to the same L2 block, only the first one may have non-zero `_maxVirtualBlocksToCreate`. The rest of the data must be same.

The `setL2Block` [performs](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/SystemContext.sol#L355) a lot of similar consistency checks to the ones for the L1 batch.

#### L2 blockhash calculation and storage

Unlike L1 batch’s hash, the L2 blocks’ hashes can be checked on L2.

The hash of an L2 block is `keccak256(abi.encode(_blockNumber, _blockTimestamp, _prevL2BlockHash, _blockTxsRollingHash))`. Where `_blockTxsRollingHash` is defined in the following way:

`_blockTxsRollingHash = 0` for an empty block.

`_blockTxsRollingHash = keccak(0, tx1_hash)` for a block with one tx.

`_blockTxsRollingHash = keccak(keccak(0, tx1_hash), tx2_hash)` for a block with two txs, etc.

To add a transaction hash to the current miniblock we use the `appendTransactionToCurrentL2Block` function of the `SystemContext` contract.

Since ZKsync is a state-diff based rollup, there is no way to deduce the hashes of the L2 blocks based on the transactions’ in the batch (because there is no access to the transaction’s hashes). At the same time, in order to execute `blockhash` method, the VM requires the knowledge of some of the previous L2 block hashes. In order to save up on pubdata (by making sure that the same storage slots are reused, i.e. we only have repeated writes) we [store](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/SystemContext.sol#L73) only the last 257 block hashes. You can read more on what are the repeated writes and how the pubdata is processed [here](../settlement_contracts/data_availability/standard_pubdata_format.md).

We store only the last 257 blocks, since the EVM requires only 256 previous ones and we use 257 as a safe margin.

#### Legacy blockhash

For L2 blocks that were created before we switched to the formulas from above, we use the following formula for their hash:

`keccak256(abi.encodePacked(uint32(_blockNumber)))`

These are only very old blocks on ZKsync Era and other ZK chains don't have such blocks.

#### Timing invariants

While the timestamp of each L2 block is provided by the operator, there are some timing invariants that the system preserves:

- For each L2 block its timestamp should be > the timestamp of the previous L2 block
- For each L2 block its timestamp should be ≥ timestamp of the batch it belongs to
- Each batch must start with a new L2 block (i.e. an L2 block can not span across batches).
- The timestamp of a batch must be ≥ the timestamp of the latest L2 block which belonged to the previous batch.
- The timestamp of the last miniblock in batch can not go too far into the future. This is enforced by publishing an L2→L1 log, with the timestamp which is then checked on L1.

### Fictive L2 block & finalizing the batch

At the end of the batch, the bootloader calls the `setL2Block` [one more time](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L4110) to allow the operator to create a new empty block. This is done purely for some of the technical reasons inside the node, where each batch ends with an empty L2 block.

We do not enforce that the last block is empty explicitly as it complicates the development process and testing, but in practice, it is, and either way, it should be secure.

Also, at the end of the batch we send the timestamps of the batch as well as the timestamp of the last miniblock in order to check on L1 that both of these are realistic. Checking any other L2 block’s timestamp is not required since all of them are enforced to be between those two.

## Additional note on blockhashes

In the past, we had to apply different formulas based on whether or not the migration from batch environment info to L2 block info has finished. You can find these checks [here](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/SystemContext.sol#L137). But note, that the migration has ended quite some time ago, so in reality only the two cases above can be met:

- When the block is out of the readable range.
- When it is a normal L2 block and so its hash has to be used.

The only edge case is when we ask for miniblock block number for which the base hash is returned. This edge case will be removed in future releases.
