# Batches & L2 blocks on ZKsync

## Glossary

- **Batch** – a set of transactions that the bootloader processes (`commitBatches`, `proveBatches`, and `executeBatches` work on it). A batch consists of multiple transactions.  
- **L2 blocks** – non-intersecting subsets of consecutively executed transactions in a batch. This is the kind of block you see in the API and the one used for `block.number`/`block.timestamp`/etc.

> Note that sometimes in code you can see the notion of “virtual blocks.” In the past, we returned batch information for `block.number`/`block.timestamp`. However, due to DevEx issues, we decided to move to returning these values for L2 blocks. Virtual blocks were used during migration but are not used anymore. You should consider that there is one virtual block per L2 block and it has exactly the same properties.

## Motivation

L2 blocks were created for fast soft confirmation in wallets and block explorers. For example, MetaMask shows transactions as confirmed only after the block in which the transaction execution was mined. So, if the user needs to wait for batch confirmation, it would take at least a few minutes (for soft confirmation) and hours for full confirmation, which is very bad UX. But the API can return soft confirmation much earlier through L2 blocks.

## Adapting for Solidity

In order to get the returned value for `block.number`, `block.timestamp`, `blockhash`, our compiler used the following functions:

- `getBlockNumber`  
- `getBlockTimestamp`  
- `getBlockHashEVM`  

These return values for L2 blocks.

## Blocks’ processing and consistency checks

Our `SystemContext` contract allows retrieval of information about batches and L2 blocks. Some of the information is hard to calculate on-chain—for instance, time. The timing information (for both batches and L2 blocks) is provided by the operator. To check that the operator provided realistic values, certain checks are done on L1. Generally, though, we try to check as much as we can on L2.

### Initializing L1 batch

At the start of a batch, the operator [provides](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L3935) the batch timestamp, its number, and the hash of the previous batch. The root hash of the Merkle tree serves as the root hash of the batch.

The `SystemContext` can immediately check whether the provided number is the correct batch number. It also immediately sends the previous batch hash to L1, where it will be checked during the commit operation. Additionally, some general consistency checks are performed. This logic can be found [here](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/SystemContext.sol#L469).

### L2 blocks processing and consistency checks

#### `setL2Block`

Before each transaction, we call the `setL2Block` [method](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L2884). Here, we provide data about the L2 block that the transaction belongs to:

- `_l2BlockNumber` – the number of the new L2 block.  
- `_l2BlockTimestamp` – the timestamp of the new L2 block.  
- `_expectedPrevL2BlockHash` – the expected hash of the previous L2 block.  
- `_isFirstInBatch` – whether this method is called for the first time in the batch.  
- `_maxVirtualBlocksToCreate` – the maximum number of virtual blocks to create with this L2 block. This is a legacy field that is always either 0 or 1.

If two transactions belong to the same L2 block, only the first one may have a non-zero `_maxVirtualBlocksToCreate`. The rest of the data must be the same.

The `setL2Block` [performs](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/SystemContext.sol#L355) many consistency checks similar to those for the L1 batch.

#### L2 blockhash calculation and storage

Unlike the L1 batch’s hash, L2 block hashes can be checked on-chain. The hash of an L2 block is computed as:  

The hash of an L2 block is `keccak256(abi.encode(_blockNumber, _blockTimestamp, _prevL2BlockHash, _blockTxsRollingHash))`. Where `_blockTxsRollingHash` is defined in the following way:

- `_blockTxsRollingHash = 0` for an empty block.  
- `_blockTxsRollingHash = keccak256(abi.encodePacked(0, tx1_hash))` for a block with one transaction.  
- `_blockTxsRollingHash = keccak256(abi.encodePacked(keccak256(abi.encodePacked(0, tx1_hash)), tx2_hash))` for a block with two transactions, etc.

To add a transaction hash to the current miniblock, we use the `appendTransactionToCurrentL2Block` function of the `SystemContext` contract.

Since ZKsync is a state-diff-based rollup, there is no way to deduce L2 block hashes based on the transactions in the batch (because there is no access to the transaction hashes). At the same time, to execute the `blockhash` method, the VM requires knowledge of some previous L2 block hashes. To save on pubdata (by reusing the same storage slots, i.e. only having repeated writes), we [store](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/SystemContext.sol#L73) only the last 257 block hashes. You can read more about repeated writes and how pubdata is processed [here](../settlement_contracts/data_availability/standard_pubdata_format.md).

We store only the last 257 blocks because the EVM requires only 256 previous hashes, and we use 257 as a safe margin.

#### Legacy blockhash

For L2 blocks that were created before we switched to the formulas above, we use the following formula for their hash:  

`keccak256(abi.encodePacked(uint32(_blockNumber)))`

These are only very old blocks on ZKsync Era; other ZK chains do not have such blocks.

#### Timing invariants

While the timestamp of each L2 block is provided by the operator, the system preserves these timing invariants:

- For each L2 block, its timestamp should be greater than the timestamp of the previous L2 block.  
- For each L2 block, its timestamp should be ≥ the timestamp of the batch it belongs to.  
- Each batch must start with a new L2 block (i.e. an L2 block cannot span across batches).  
- The timestamp of a batch must be ≥ the timestamp of the latest L2 block from the previous batch.  
- The timestamp of the last miniblock in a batch cannot go too far into the future. This is enforced by publishing an L2→L1 log with the timestamp, which is then checked on L1.

### Fictive L2 block & finalizing the batch

At the end of a batch, the bootloader calls `setL2Block` [one more time](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L4110) to allow the operator to create a new empty block. This is done for technical reasons inside the node, where each batch ends with an empty L2 block.

We do not enforce that the last block is empty explicitly, as that complicates development and testing. In practice, it is empty, and either way, the system remains secure.

Also, at the end of a batch, we send both the batch timestamp and the timestamp of the last miniblock so that L1 can verify those two values are realistic. Checking any other L2 block’s timestamp is not required since all of them are enforced to lie between those two.

## Additional note on blockhashes

In the past, we had to apply different formulas based on whether the migration from batch environment info to L2 block info had finished. You can find these checks [here](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/SystemContext.sol#L137). But note that the migration ended quite some time ago, so only two cases remain:

- When the block is out of the readable range.  
- When it is a normal L2 block, and its hash must be used.

The only edge case is when we ask for a miniblock block number for which the base hash is returned. This edge case will be removed in future releases.
