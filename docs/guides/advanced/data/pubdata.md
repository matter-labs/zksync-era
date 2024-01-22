# Overview

Pubdata in zkSync can be divided up into 4 different categories:

1. L2 to L1 Logs
2. L2 to L1 Messages
3. Smart Contract Bytecodes
4. Storage writes

Using data corresponding to these 4 facets, across all executed batches, we’re able to reconstruct the full state of L2.
One thing to note is that the way that the data is represented changes in a pre-boojum and post-boojum zkEVM. At a high
level, in a pre-boojum era these are represented as separate fields while in boojum they are packed into a single bytes
array.

> Note: Once 4844 gets integrated this bytes array will move from being part of the calldata to blob data.

While the structure of the pubdata changes, we can use the same strategy to pull the relevant information. First, we
need to filter all of the transactions to the L1 zkSync contract for only the `commitBlocks` transactions where the
proposed block has been referenced by a corresponding `executeBlocks` call (the reason for this is that a committed or
even proven block can be reverted but an executed one cannot). Once we have all the committed blocks that have been
executed, we then will pull the transaction input and the relevant fields, applying them in order to reconstruct the
current state of L2.

One thing to note is that in both systems some of the contract bytecode is compressed into an array of indices where
each 2 byte index corresponds to an 8 byte word in a dictionary. More on how that is done [here](./compression.md). Once
the bytecode has been expanded, the hash can be taken and checked against the storage writes within the
`AccountCodeStorage` contract which connects an address on L2 with the 32 byte code hash:

```solidity
function _storeCodeHash(address _address, bytes32 _hash) internal {
  uint256 addressAsKey = uint256(uint160(_address));
  assembly {
    sstore(addressAsKey, _hash)
  }
}

```

### Pre-Boojum Era

In pre-boojum era the superset of pubdata fields and input to the `commitBlocks` function follows the following format:

```solidity
/// @notice Data needed to commit new block
/// @param blockNumber Number of the committed block
/// @param timestamp Unix timestamp denoting the start of the block execution
/// @param indexRepeatedStorageChanges The serial number of the shortcut index that's used as a unique identifier for storage keys that were used twice or more
/// @param newStateRoot The state root of the full state tree
/// @param numberOfLayer1Txs Number of priority operations to be processed
/// @param l2LogsTreeRoot The root hash of the tree that contains all L2 -> L1 logs in the block
/// @param priorityOperationsHash Hash of all priority operations from this block
/// @param initialStorageChanges Storage write access as a concatenation key-value
/// @param repeatedStorageChanges Storage write access as a concatenation index-value
/// @param l2Logs concatenation of all L2 -> L1 logs in the block
/// @param l2ArbitraryLengthMessages array of hash preimages that were sent as value of L2 logs by special system L2 contract
/// @param factoryDeps (contract bytecodes) array of L2 bytecodes that were deployed
struct CommitBlockInfo {
  uint64 blockNumber;
  uint64 timestamp;
  uint64 indexRepeatedStorageChanges;
  bytes32 newStateRoot;
  uint256 numberOfLayer1Txs;
  bytes32 l2LogsTreeRoot;
  bytes32 priorityOperationsHash;
  bytes initialStorageChanges;
  bytes repeatedStorageChanges;
  bytes l2Logs;
  bytes[] l2ArbitraryLengthMessages;
  bytes[] factoryDeps;
}

```

The 4 main fields to look at here are:

1. `initialStorageChanges`: Storage slots being written to for the first time and the corresponding value
   1. Structure: `num entries as u32 || for each entry: (32 bytes key, 32 bytes final value)`
2. `repeatedStorageChanges`: ids of the slots being written to and the corresponding value
   1. Structure: `num entries as u32 || for each entry: (8 byte id, 32 bytes final value)`
3. `factoryDeps`: An array of uncompressed bytecodes
4. `l2ArbitraryLengthMessages` : L2 → L1 Messages
   1. We don’t need them all, we are just concerned with messages sent from the `Compressor/BytecodeCompressor` contract
   2. These messages will follow the compression algorithm outline [here](./compression.md)

For the ids on the repeated writes, they are generated as we process the first time keys. For example: if we see
`[<key1, val1>, <key2, val2>]` (starting from an empty state) then we can assume that the next time a write happens to
`key1` it will be encoded as `<1, new_val>` and so on and so forth. There is a little shortcut here where the last new
id generated as part of a batch will be in the `indexRepeatedStorageChanges` field.

### Post-Boojum Era

```solidity
/// @notice Data needed to commit new block
/// @param blockNumber Number of the committed block
/// @param timestamp Unix timestamp denoting the start of the block execution
/// @param indexRepeatedStorageChanges The serial number of the shortcut index that's used as a unique identifier for storage keys that were used twice or more
/// @param newStateRoot The state root of the full state tree
/// @param numberOfLayer1Txs Number of priority operations to be processed
/// @param priorityOperationsHash Hash of all priority operations from this block
/// @param systemLogs concatenation of all L2 -> L1 system logs in the block
/// @param totalL2ToL1Pubdata Total pubdata committed to as part of bootloader run. Contents are: l2Tol1Logs <> l2Tol1Messages <> publishedBytecodes <> stateDiffs
struct CommitBlockInfo {
  uint64 blockNumber;
  uint64 timestamp;
  uint64 indexRepeatedStorageChanges;
  bytes32 newStateRoot;
  uint256 numberOfLayer1Txs;
  bytes32 priorityOperationsHash;
  bytes systemLogs;
  bytes totalL2ToL1Pubdata;
}

```

The main difference between the two `CommitBlockInfo` structs is that we have taken a few of the fields and merged them
into a single bytes array called `totalL2ToL1Pubdata`. The contents of pubdata include:

1. L2 to L1 Logs
2. L2 to L1 Messages
3. Published Bytecodes
4. Compressed State Diffs

The 2 main fields needed for state reconstruction are the bytecodes and the state diffs. The bytecodes follow the same
structure and reasoning in the old system (as explained above). The state diffs will follow the compression illustrated
below.

### Compression of State Diffs in Post-Boojum Era

#### Keys

Keys will be packed in the same way as they were before boojum. The only change is that we’ll avoid using the 8-byte
enumeration index and will pack it to the minimal necessary number of bytes. This number will be part of the pubdata.
Once a key has been used, it can already use the 4 or 5 byte enumeration index and it is very hard to have something
cheaper for keys that has been used already. The opportunity comes when remembering the ids for accounts to spare some
bytes on nonce/balance key, but ultimately the complexity may not be worth it.

There is some room for the keys that are being written for the first time, however, these are rather more complex and
achieve only a one-time effect (when the key is published for the first time).

#### Values

Values are much easier to compress, since they usually contain only zeroes. Also, we can leverage the nature of how
those values are changed. For instance if nonce has been increased only by 1, we do not need to write the entire 32-byte
new value, we can just tell that the slot has been _increased_ and then supply only 1-byte value of _the size by which_
it was increased. This way instead of 32 bytes we need to publish only 2 bytes: first byte to denote which operation has
been applied and the second by to denote the size by which the addition has been made.

If we decide to have just the following 4 types of changes: `Add`, `Sub,` `Transform`, `NoCompression` where:

- `Add` denotes that the value has been increased. (modulo 2^256)
- `Sub` denotes that the value has been decreased. (modulo 2^256)
- `Transform` denotes the value just has been changed (i.e. we disregard any potential relation between the previous and
  the new value, though the new value might be small enough to save up on the number of bytes).
- `NoCompression` denotes that the whole 32 byte value will be used.

Where the byte size of the output can be anywhere from 0 to 31 (also 0 makes sense for `Transform`, since it denotes
that it has been zeroed out). For `NoCompression` the whole 32 byte value is used.

So the format of the pubdata will be the following:

##### Part 1. Header

- `<version = 1 byte>` — this will enable easier automated unpacking in the future. Currently, it will be only equal to
  `1`.
- `<total_logs_len = 3 bytes>` — we need only 3 bytes to describe the total length of the L2→L1 logs.
- `<the number of bytes used for derived keys = 1 byte>`. At the beginning it will be equal to `4`, but then it will
  automatically switch to `5` when needed.

##### Part 2. Initial writes

- `<num_of_initial_writes = 2 bytes>` (since each initial write publishes at least 32 bytes for key, then
  `2^16 * 32 = 2097152` will be enough for a lot of time (right now with the limit of 120kb it will take more than 15 L1
  txs to use up all the space there).
- Then for each `<key, value>` pair for each initial write:
  - print key as 32-byte derived key.
  - packing type as a 1 byte value, which consists of 5 bits to denote the length of the packing and 3 bits to denote
    the type of the packing (either `Add`, `Sub`, `Transform` or `NoCompression`). More on it
    [below](https://www.notion.so/Pubdata-compression-v1-4b0dd8c151014c8ab96dbd7e66e17599?pvs=21).
  - The packed value itself.

#### Part 3. Repeated writes

Note, that there is no need to write the number of repeated writes, since we know that until the end of the pubdata, all
the writes will be repeated ones.

- For each `<key, value>` pair for each repeated write:
  - print key as either 4 or 5 byte derived key.
  - packing type as a 1 byte value, which consists of 5 bits to denote the length of the packing and 3 bits to denote
    the type of the packing (either `Add`, `Sub`, `Transform` or `NoCompression`).
  - The packed value itself.
