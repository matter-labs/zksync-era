# Handling pubdata in Boojum

Pubdata in ZKsync can be divided up into 4 different categories:

1. L2 to L1 Logs
2. L2 to L1 Messages
3. Smart Contract Bytecodes
4. Storage writes

Using data corresponding to these 4 facets, across all executed batches, we’re able to reconstruct the full state of L2.
With the upgrade to our new proof system, Boojum, the way this data is represented will change. At a high level, in the
pre-Boojum system these are represented as separate fields while for boojum they will be packed into a single bytes
array. Once 4844 gets integrated this bytes array will move from being part of the calldata to blob data.

While the structure of the pubdata changes, the way in which one can go about pulling the information will remain the
same. Basically, we just need to filter all of the transactions to the L1 ZKsync contract for only the `commitBatches`
transactions where the proposed block has been referenced by a corresponding `executeBatches` call (the reason for this
is that a committed or even proven block can be reverted but an executed one cannot). Once we have all the committed
batches that have been executed, we then will pull the transaction input and the relevant fields, applying them in order
to reconstruct the current state of L2.

## L2→L1 communication

### L2→L1 communication before Boojum

While there were quite some changes during Boojum upgrade, most of the scheme remains the same and so explaining how it
worked before gives some background on why certain decisions are made and kept for backward compatibility.

[L2→L1 communication before Boojum](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Smart%20contract%20Section/Handling%20pubdata%20in%20Boojum/L2%E2%86%92L1%20communication%20before%20Boojum.md)

The most important feature that we’ll need to maintain in Boojum for backward compatibility is to provide a similar
Merkle tree of L2→L1 logs with the long L2→L1 messages and priority operations’ status.

Before Boojum, whenever we sent an L2→L1 long message, a _log_ was appended to the Merkle tree of L2→L1 messages on L1
due to necessity. In Boojum we’ll have to maintain this fact. Having the priority operations’ statuses is important to
enable
[proving](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/contracts/ethereum/contracts/bridge/L1ERC20Bridge.sol#L255)
failed deposits for bridges.

### Changes with Boojum

#### Problems with the previous approach

- There was a limit of 512 L2→L1 logs per batch, which is very limiting. It causes our block to be forcefully closed
  based on the number of these messages instead of having the pubdata as the only limit.
- In the ideal world, we would like to have the tree adapt to the requirements of the batch, with any number of leaves
  possible (in practice, a maximum of 2048 would likely be enough for the foreseeable future).
- Extending the tree in the circuits will be hard to do and hard to maintain.
- The hash of the contents of the L2→L1 messages needs to be rehashed to support the danksharding blobs, so we want to
  keep only the essential logs as parts of calldata and the rest should be separated so that they could be moved the
  EIP4844 blob in the future.

#### Solution

We will implement the calculation of the Merkle root of the L2→L1 messages via a system contract as part of the
`L1Messenger`. Basically, whenever a new log emitted by users that needs to be Merklized is created, the `L1Messenger`
contract will append it to its rolling hash and then at the end of the batch, during the formation of the blob it will
receive the original preimages from the operator, verify, and include the logs to the blob.

We will now call the logs that are created by users and are Merklized _user_ logs and the logs that are emitted by
natively by VM _system_ logs. Here is a short comparison table for better understanding:

| System logs                                                                                                     | User logs                                                                                                                                                                                                                           |
| --------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Emitted by VM via an opcode.                                                                                    | VM knows nothing about them.                                                                                                                                                                                                        |
| Consistency and correctness is enforced by the verifier on L1 (i.e. their hash is part of the block commitment. | Consistency and correctness is enforced by the L1Messenger system contract. The correctness of the behavior of the L1Messenger is enforced implicitly by prover in a sense that it proves the correctness of the execution overall. |
| We don’t calculate their Merkle root.                                                                           | We calculate their Merkle root on the L1Messenger system contract.                                                                                                                                                                  |
| We have constant small number of those.                                                                         | We can have as much as possible as long as the commitBatches function on L1 remains executable (it is the job of the operator to ensure that only such transactions are selected)                                                   |
| In EIP4844 they will remain part of the calldata.                                                               | In EIP4844 they will become part of the blobs.                                                                                                                                                                                      |

#### Backwards-compatibility

Note, that to maintain a unified interface with the previous version of the protocol, the leaves of the Merkle tree will
have to maintain the following structure:

```solidity
struct L2Log {
  uint8 l2ShardId;
  bool isService;
  uint16 txNumberInBlock;
  address sender;
  bytes32 key;
  bytes32 value;
}

```

While the leaf will look the following way:

```solidity
bytes32 hashedLog = keccak256(
    abi.encodePacked(_log.l2ShardId, _log.isService, _log.txNumberInBlock, _log.sender, _log.key, _log.value)
);
```

`keccak256` will continue being the function for the merkle tree.

To put it shortly, the proofs for L2→L1 log inclusion will continue having exactly the same format as they did in the
pre-Boojum system, which avoids breaking changes for SDKs and bridges alike.

#### Implementation of `L1Messenger`

The L1Messenger contract will maintain a rolling hash of all the L2ToL1 logs `chainedLogsHash` as well as the rolling
hashes of messages `chainedMessagesHash`. Whenever a contract wants to send an L2→L1 log, the following operation will
be
[applied](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/L1Messenger.sol#L110):

`chainedLogsHash = keccak256(chainedLogsHash, hashedLog)`. L2→L1 logs have the same 88-byte format as in the current
version of ZKsync.

Note, that the user is charged for necessary future the computation that will be needed to calculate the final merkle
root. It is roughly 4x higher than the cost to calculate the hash of the leaf, since the eventual tree might have be 4x
times the number nodes. In any case, this will likely be a relatively negligible part compared to the cost of the
pubdata.

At the end of the execution, the bootloader will
[provide](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L2470)
a list of all the L2ToL1 logs as well as the messages in this block to the L1Messenger (this will be provided by the
operator in the memory of the bootloader). The L1Messenger checks that the rolling hash from the provided logs is the
same as in the `chainedLogsHash` and calculate the merkle tree of the provided messages. Right now, we always build the
Merkle tree of size `2048`, but we charge the user as if the tree was built dynamically based on the number of leaves in
there. The implementation of the dynamic tree has been postponed until the later upgrades.

#### Long L2→L1 messages & bytecodes

Before, the fact that the correct preimages for L2→L1 messages as bytecodes were provided was checked on the L1 side.
Now, it will be done on L2.

If the user wants to send an L2→L1 message, its preimage is
[appended](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/L1Messenger.sol#L125)
to the message’s rolling hash too `chainedMessagesHash = keccak256(chainedMessagesHash, keccak256(message))`.

A very similar approach for bytecodes is used, where their rolling hash is calculated and then the preimages are
provided at the end of the batch to form the full pubdata for the batch.

Note, that in for backward compatibility, just like before any long message or bytecode is accompanied by the
corresponding user L2→L1 log.

#### Using system L2→L1 logs vs the user logs

The content of the L2→L1 logs by the L1Messenger will go to the blob of EIP4844. Meaning, that all the data that belongs
to the tree by L1Messenger’s L2→L1 logs should not be needed during block commitment. Also, note that in the future we
will remove the calculation of the Merkle root of the built-in L2→L1 messages.

The only places where the built-in L2→L1 messaging should continue to be used:

- Logs by SystemContext (they are needed on commit to check the previous block hash).
- Logs by L1Messenger for the merkle root of the L2→L1 tree as well as the hash of the `totalPubdata`.
- `chainedPriorityTxsHash` and `numberOfLayer1Txs` from the bootloader (read more about it below).

#### Obtaining `txNumberInBlock`

To have the same log format, the `txNumberInBlock` must be obtained. While it is internally counted in the VM, there is
currently no opcode to retrieve this number. We will have a public variable `txNumberInBlock` in the `SystemContext`,
which will be incremented with each new transaction and retrieve this variable from there. It is
[zeroed out](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/SystemContext.sol#L458)
at the end of the batch.

### Bootloader implementation

The bootloader has a memory segment dedicated to the ABI-encoded data of the L1ToL2Messenger to perform the
`publishPubdataAndClearState` call.

At the end of the execution of the batch, the operator should provide the corresponding data into the bootloader memory,
i.e user L2→L1 logs, long messages, bytecodes, etc. After that, the
[call](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L2484)
is performed to the `L1Messenger` system contract, that should validate the adherence of the pubdata to the required
format

## Bytecode Publishing

Within pubdata, bytecodes are published in 1 of 2 ways: (1) uncompressed via `factoryDeps` (pre-boojum this is within
its own field, and post-boojum as part of the `totalPubdata`) and (2) compressed via long l2 → l1 messages.

### Uncompressed Bytecode Publishing

With Boojum, `factoryDeps` are included within the `totalPubdata` bytes and have the following format:
`number of bytecodes || forEachBytecode (length of bytecode(n) || bytecode(n))` .

### Compressed Bytecode Publishing

This part stays the same in a pre and post boojum ZKsync. Unlike uncompressed bytecode which are published as part of
`factoryDeps`, compressed bytecodes are published as long l2 → l1 messages which can be seen
[here](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/Compressor.sol#L80).

#### Bytecode Compression Algorithm — Server Side

This is the part that is responsible for taking bytecode, that has already been chunked into 8 byte words, performing
validation, and compressing it.

Each 8 byte word from the chunked bytecode is assigned a 2 byte index (constraint on size of dictionary of chunk → index
is 2^16 - 1 elements). The length of the dictionary, dictionary entries (index assumed through order), and indexes are
all concatenated together to yield the final compressed version.

For bytecode to be considered valid it must satisfy the following:

1. Bytecode length must be less than 2097120 ((2^16 - 1) \* 32) bytes.
2. Bytecode length must be a multiple of 32.
3. Number of 32-byte words cannot be even.

The following is a simplified version of the algorithm:

```python
statistic: Map[chunk, (count, first_pos)]
dictionary: Map[chunk, index]
encoded_data: List[index]

for position, chunk in chunked_bytecode:
 if chunk is in statistic:
  statistic[chunk].count += 1
 else:
  statistic[chunk] = (count=1, first_pos=pos)

# We want the more frequently used bytes to have smaller ids to save on calldata (zero bytes cost less)
statistic.sort(primary=count, secondary=first_pos, order=desc)

for index, chunk in enumerated(sorted_statistics):
  dictionary[chunk] = index

for chunk in chunked_bytecode:
 encoded_data.append(dictionary[chunk])

return [len(dictionary), dictionary.keys(order=index asc), encoded_data]
```

#### Verification And Publishing — L2 Contract

The function `publishCompressBytecode` takes in both the original `_bytecode` and the `_rawCompressedData` , the latter
of which comes from the output of the server’s compression algorithm. Looping over the encoded data, derived from
`_rawCompressedData` , the corresponding chunks are pulled from the dictionary and compared to the original byte code,
reverting if there is a mismatch. After the encoded data has been verified, it is published to L1 and marked accordingly
within the `KnownCodesStorage` contract.

Pseudo-code implementation:

```python
length_of_dict = _rawCompressedData[:2]
dictionary = _rawCompressedData[2:2 + length_of_dict * 8] # need to offset by bytes used to store length (2) and multiply by 8 for chunk size
encoded_data = _rawCompressedData[2 + length_of_dict * 8:]

assert(len(dictionary) % 8 == 0) # each element should be 8 bytes
assert(num_entries(dictionary) <= 2^16)
assert(len(encoded_data) * 4 == len(_bytecode)) # given that each chunk is 8 bytes and each index is 2 bytes they should differ by a factor of 4

for (index, dict_index) in list(enumerate(encoded_data)):
 encoded_chunk = dictionary[dict_index]
 real_chunk = _bytecode.readUint64(index * 8) # need to pull from index * 8 to account for difference in element size
 verify(encoded_chunk == real_chunk)

# Sending the compressed bytecode to L1 for data availability
sendToL1(_rawCompressedBytecode)
markAsPublished(hash(_bytecode))
```

## Storage diff publishing

ZKsync is a statediff-based rollup and so publishing the correct state diffs plays an integral role in ensuring data
availability.

### How publishing of storage diffs worked before Boojum

As always in order to understand the new system better, some information about the previous one is important.

Before, the system contracts had no clue about storage diffs. It was the job of the operator to provide the
`initialStorageChanges` and `reapeatedStorageWrites` (more on the differences will be explained below). The information
to commit the block looked the following way:

```solidity
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

These two fields would be then included into the block commitment and checked by the verifier.

### Difference between initial and repeated writes

ZKsync publishes state changes that happened within the batch instead of transactions themselves. Meaning, that for
instance some storage slot `S` under account `A` has changed to value `V`, we could publish a triple of `A,S,V`. Users
by observing all the triples could restore the state of ZKsync. However, note that our tree unlike Ethereum’s one is not
account based (i.e. there is no first layer of depth 160 of the merkle tree corresponding to accounts and second layer
of depth 256 of the merkle tree corresponding to users). Our tree is “flat”, i.e. a slot `S` under account `A` is just
stored in the leaf number `H(S,A)`. Our tree is of depth 256 + 8 (the 256 is for these hashed account/key pairs and 8 is
for potential shards in the future, we currently have only one shard and it is irrelevant for the rest of the document).

We call this `H(S,A)` _derived key_, because it is derived from the address and the actual key in the storage of the
account. Since our tree is flat, whenever a change happens, we can publish a pair `DK, V`, where `DK=H(S,A)`.

However, these is an optimization that could be done:

- Whenever a change to a key is used for the first time, we publish a pair of `DK,V` and we assign some sequential id to
  this derived key. This is called an _initial write_. It happens for the first time and that’s why we must publish the
  full key.
- If this storage slot is published in some of the subsequent batches, instead of publishing the whole `DK`, we can use
  the sequential id instead. This is called a _repeated write_.

For instance, if the slots `A`,`B` (I’ll use latin letters instead of 32-byte hashes for readability) changed their
values to `12`,`13` accordingly, in the batch it happened they will be published in the following format:

- `(A, 12), (B, 13)`. Let’s say that the last sequential id ever used is 6. Then, `A` will receive the id of `7` and B
  will receive the id of `8`.

Let’s say that in the next block, they changes their values to `13`,`14`. Then, their diff will be published in the
following format:

- `(7, 13), (8,14)`.

The id is permanently assigned to each storage key that was ever published. While in the description above it may not
seem like a huge boost, however, each `DK` is 32 bytes long and id is at most 8 bytes long.

We call this id _enumeration_index_.

Note, that the enumeration indexes are assigned in the order of sorted array of (address, key), i.e. they are internally
sorted. The enumeration indexes are part of the state merkle tree, it is **crucial** that the initial writes are
published in the correct order, so that anyone could restore the correct enum indexes for the storage slots. In
addition, an enumeration index of `0` indicates that the storage write is an initial write.

### State diffs after Boojum upgrade

Firstly, let’s define what we’ll call the `stateDiffs`. A _state diff_ is an element of the following structure.

[https://github.com/matter-labs/era-zkevm_test_harness/blob/3cd647aa57fc2e1180bab53f7a3b61ec47502a46/circuit_definitions/src/encodings/state_diff_record.rs#L8](https://github.com/matter-labs/era-zkevm_test_harness/blob/3cd647aa57fc2e1180bab53f7a3b61ec47502a46/circuit_definitions/src/encodings/state_diff_record.rs#L8).

Basically, it contains all the values which might interest us about the state diff:

- `address` where the storage has been changed.
- `key` (the original key inside the address)
- `derived_key` — `H(key, address)` as described in the previous section.
  - Note, the hashing algorithm currently used here is `Blake2s`
- `enumeration_index` — Enumeration index as explained above. It is equal to 0 if the write is initial and contains the
  non-zero enumeration index if it is the repeated write (indexes are numerated starting from 1).
- `initial_value` — The value that was present in the key at the start of the batch
- `final_value` — The value that the key has changed to by the end of the batch.

We will consider `stateDiffs` an array of such objects, sorted by (address, key).

This is the internal structure that is used by the circuits to represent the state diffs. The most basic “compression”
algorithm is the one described above:

- For initial writes, write the pair of (`derived_key`, `final_value`)
- For repeated writes write the pair of (`enumeration_index`, `final_value`).

Note, that values like `initial_value`, `address` and `key` are not used in the "simplified" algorithm above, but they
will be helpful for the more advanced compression algorithms in the future. The
[algorithm](#state-diff-compression-format) for Boojum will already utilize the difference between the `initial_value`
and `final_value` for saving up on pubdata.

### How the new pubdata verification would work

#### L2

1. The operator provides both full `stateDiffs` (i.e. the array of the structs above) and the compressed state diffs
   (i.e. the array which contains the state diffs, compressed by the algorithm explained
   [below](#state-diff-compression-format)).
2. The L1Messenger must verify that the compressed version is consistent with the original stateDiffs.
3. Once verified, the L1Messenger will publish the _hash_ of the original state diff via a system log. It will also
   include the compressed state diffs into the totalPubdata to be published onto L1.

#### L1

1. During committing the block, the L1
   [verifies](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/contracts/ethereum/contracts/zksync/facets/Executor.sol#L139)
   that the operator has provided the full preimage for the totalPubdata (which includes L2→L1 logs, L2→L1 messages,
   bytecodes as well as the compressed state diffs).
2. The block commitment
   [includes](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/contracts/ethereum/contracts/zksync/facets/Executor.sol#L462)
   \*the hash of the `stateDiffs`. Thus, during ZKP verification will fail if the provided stateDiff hash is not
   correct.

It is a secure construction because the proof can be verified only if both the execution was correct and the hash of the
provided hash of the `stateDiffs` is correct. This means that the L1Messenger indeed received the array of correct
`stateDiffs` and, assuming the L1Messenger is working correctly, double-checked that the compression is of the correct
format, while L1 contracts on the commit stage double checked that the operator provided the preimage for the compressed
state diffs.

### State diff compression format

The following algorithm is used for the state diff compression:

[State diff compression](./compression.md)

## General pubdata format

At the end of the execution of the batch, the bootloader provides the `L1Messenger` with the preimages for the user
L2→L1 logs, L2→L1 long messages as well as uncompressed bytecodes. It also provides with compressed state diffs as well
as the original expanded state diff entries.

It will check that the preimages are correct as well as the fact that the compression is correct. It will output the
following three values via system logs:

- The root of the L2→L1 log Merkle tree. It will be stored and used for proving withdrawals.
- The hash of the `totalPubdata` (i.e. the pubdata that contains the preimages above as well as packed state diffs).
- The hash of the state diffs provided by the operator (it later on be included in the block commitment and its will be
  enforced by the circuits).

The `totalPubdata` has the following structure:

1. First 4 bytes — the number of user L2→L1 logs in the batch
2. Then, the concatenation of packed L2→L1 user logs.
3. Next, 4 bytes — the number of long L2→L1 messages in the batch.
4. Then, the concatenation of L2→L1 messages, each in the format of `<4 byte length || actual_message>`.
5. Next, 4 bytes — the number of uncompressed bytecodes in the batch.
6. Then, the concatenation of uncompressed bytecodes, each in the format of `<4 byte length || actual_bytecode>`.
7. Next, 4 bytes — the length of the compressed state diffs.
8. Then, state diffs are compressed by the spec [above](#state-diff-compression-format).

With Boojum, the interface for committing batches is the following one:

```solidity
/// @notice Data needed to commit new batch
/// @param batchNumber Number of the committed batch
/// @param timestamp Unix timestamp denoting the start of the batch execution
/// @param indexRepeatedStorageChanges The serial number of the shortcut index that's used as a unique identifier for storage keys that were used twice or more
/// @param newStateRoot The state root of the full state tree
/// @param numberOfLayer1Txs Number of priority operations to be processed
/// @param priorityOperationsHash Hash of all priority operations from this batch
/// @param bootloaderHeapInitialContentsHash Hash of the initial contents of the bootloader heap. In practice it serves as the commitment to the transactions in the batch.
/// @param eventsQueueStateHash Hash of the events queue state. In practice it serves as the commitment to the events in the batch.
/// @param systemLogs concatenation of all L2 -> L1 system logs in the batch
/// @param totalL2ToL1Pubdata Total pubdata committed to as part of bootloader run. Contents are: l2Tol1Logs <> l2Tol1Messages <> publishedBytecodes <> stateDiffs
struct CommitBatchInfo {
  uint64 batchNumber;
  uint64 timestamp;
  uint64 indexRepeatedStorageChanges;
  bytes32 newStateRoot;
  uint256 numberOfLayer1Txs;
  bytes32 priorityOperationsHash;
  bytes32 bootloaderHeapInitialContentsHash;
  bytes32 eventsQueueStateHash;
  bytes systemLogs;
  bytes totalL2ToL1Pubdata;
}

```
