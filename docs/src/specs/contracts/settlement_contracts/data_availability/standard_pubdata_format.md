# Standard pubdata format

With the introduction of [custom DA validation](./custom_da.md), different pubdata logic can be applied for each chain (including calldata-based pubdata); however, ZK chains are generally optimized for a state-diffs–based rollup model.

This document describes how the standard pubdata format looks. This is the format that is enforced for [permanent rollup chains](../../chain_management/admin_role.md#ispermanentrollup-setting).

Pubdata in ZKsync can be divided up into 4 categories:

1. L2 to L1 Logs  
2. L2 to L1 Messages  
3. Smart Contract Bytecodes  
4. Storage Writes  

Using data corresponding to these 4 facets across all executed batches, we’re able to reconstruct the full state of L2. To restore the state we just need to filter all of the transactions to the L1 ZKsync contract for only the `commitBatches` transactions where the proposed block has been referenced by a corresponding `executeBatches` call (the reason for this is that a committed or even proven block can be reverted but an executed one cannot). Once we have all the committed batches that have been executed, we will then pull the transaction input and the relevant fields, applying them in order to reconstruct the current state of L2.

## L2→L1 communication

We will implement the calculation of the Merkle root of the L2→L1 messages via a system contract as part of the `L1Messenger`. Basically, whenever a new user-emitted log that needs to be Merklized is created, the `L1Messenger` contract will append it to its rolling hash and then, at the end of the batch during the formation of the blob, it will receive the original preimages from the operator, verify their consistency, and send those to the `L2DAValidator` library to facilitate the DA protocol.

We will now refer to the logs that are created by users and Merklized as _user_ logs, and the logs that are emitted natively by the VM as _system_ logs. Here is a short comparison table for better understanding:

| System logs                                                                                                     | User logs                                                                                                                                                                                                                           |
| --------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Emitted by VM via an opcode.                                                                                    | VM knows nothing about them.                                                                                                                                                                                                        |
| Consistency and correctness are enforced by the verifier on L1 (i.e. their hash is part of the block commitment). | Consistency and correctness is enforced by the L1Messenger system contract. The correctness of the behavior of the L1Messenger is enforced implicitly by the prover in the sense that it proves the correctness of the execution overall. |
| We don’t calculate their Merkle root.                                                                           | We calculate their Merkle root on the L1Messenger system contract.                                                                                                                                                                  |
| There is a constant small number of these logs.                                                                 | We can have as many as possible as long as the commitBatches function on L1 remains executable (it is the job of the operator to ensure that only such transactions are selected).                                                   |
| In EIP-4844 they will remain part of the calldata.                                                              | In EIP-4844 they will become part of the blobs.                                                                                                                                                                                     |

### Backwards-compatibility

Note that to maintain a unified interface with the previous version of the protocol, the leaves of the Merkle tree will have to maintain the following structure:

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

The leaf will look as follows:

```solidity
bytes32 hashedLog = keccak256(
    abi.encodePacked(_log.l2ShardId, _log.isService, _log.txNumberInBlock, _log.sender, _log.key, _log.value)
);
```

`keccak256` will continue being the function for the Merkle tree.

To put it shortly, the proofs for L2→L1 log inclusion will continue to have exactly the same format as they did in the pre-Boojum system, which avoids breaking changes for SDKs and bridges alike.

### Implementation of `L1Messenger`

The L1Messenger contract will maintain a rolling hash of all the L2ToL1 logs `chainedLogsHash` as well as the rolling hash of messages `chainedMessagesHash`. Whenever a contract wants to send an L2→L1 log, the following operation will be [applied](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/L1Messenger.sol#L73):

```
chainedLogsHash = keccak256(chainedLogsHash, hashedLog)
```
L2→L1 logs have the same 88-byte format as in the current version of ZKsync.

Note that the user is charged for the future computation needed to calculate the final Merkle root. It is roughly 4× higher than the cost to calculate the hash of the leaf, since the eventual tree might be 4× the number of nodes. In any case, this will likely be a relatively negligible part compared to the cost of the pubdata.

At the end of the execution, the bootloader will [provide](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L2676) a list of all the L2ToL1 logs (this will be provided by the operator in the memory of the bootloader). The L1Messenger checks that the rolling hash from the provided logs is the same as in the `chainedLogsHash` and calculates the Merkle tree of the provided messages. Right now, we always build the Merkle tree of size `16384`, but we charge the user as if the tree were built dynamically based on the number of leaves. The implementation of the dynamic tree has been postponed until later upgrades.

> Note that, unlike most other parts of pubdata, the user L2→L1 must always be validated by the trusted `L1Messenger` system contract. If we moved this responsibility to `L2DAValidator` library it would be possible for a malicious operator to provide incorrect data and forge transactions out of names of certain users.

### Long L2→L1 messages & bytecodes

If the user wants to send an L2→L1 message, its preimage is [appended](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/L1Messenger.sol#L126) to the message’s rolling hash to:

```
chainedMessagesHash = keccak256(abi.encode(chainedMessagesHash, hash));
```

A very similar approach for bytecodes is used, where their rolling hash is calculated and then the preimages are provided at the end of the batch to form the full pubdata for the batch.

Note that for backward compatibility, just like before, any long message or bytecode is accompanied by the corresponding user L2→L1 log.

### Using system L2→L1 logs vs the user logs

The content of the L2→L1 logs by the L1Messenger will go to the blob of EIP-4844. Meaning that all the data that belongs to the tree by L1Messenger’s L2→L1 logs should not be needed during block commitment. Also, note that in the future we will remove the calculation of the Merkle root of the built-in L2→L1 messages.

The only places where the built-in L2→L1 messaging should continue to be used:

- Logs by SystemContext (they are needed on commit to check the previous block hash).  
- Logs by L1Messenger for the Merkle root of the L2→L1 tree as well as the data needed for `L1DAValidator`.  
- `chainedPriorityTxsHash` and `numberOfLayer1Txs` from the bootloader (read more about it below).

### Obtaining `txNumberInBlock`

To have the same log format, the `txNumberInBlock` must be obtained. While it is internally counted in the VM, there is currently no opcode to retrieve this number. We will have a public variable `txNumberInBlock` in the `SystemContext`, which will be incremented with each new transaction and from which it can be retrieved. It is [zeroed out](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/SystemContext.sol#L515) at the end of the batch.

### Bootloader implementation

The bootloader has a memory segment dedicated to the ABI-encoded data of the L1ToL2Messenger to perform the `publishPubdataAndClearState` call.

At the end of the execution of the batch, the operator should provide the corresponding data in the bootloader memory, i.e., user L2→L1 logs, long messages, bytecodes, etc. After that, the [call](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L2676) is performed to the `L1Messenger` system contract, which would call the `L2DAValidator` library to check the adherence of the pubdata to the specified format.

## Bytecode Publishing

Within pubdata, bytecodes are published in one of two ways:  
(1) uncompressed as part of the bytecodes array and  
(2) compressed via long L2→L1 messages.

### Uncompressed Bytecode Publishing

Uncompressed bytecodes are included within the `totalPubdata` bytes and have the following format:  
```
number of bytecodes || forEachBytecode (length of bytecode(n) || bytecode(n))
```

### Compressed Bytecode Publishing

Unlike uncompressed bytecode, which is published as part of `factoryDeps`, compressed bytecodes are published as long L2→L1 messages, which can be seen [here](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/Compressor.sol#L78).

#### Bytecode Compression Algorithm — Server Side

This is the part that is responsible for taking bytecode that has already been chunked into 8-byte words, performing validation, and compressing it.

Each 8-byte word from the chunked bytecode is assigned a 2-byte index (constraint on size of dictionary of chunk → index is 2^16 – 1 elements). The length of the dictionary, dictionary entries (index assumed through order), and indexes are all concatenated together to yield the final compressed version.

For bytecode to be considered valid, it must satisfy the following:

1. Bytecode length must be less than 2097120 ((2^16 – 1) * 32) bytes.  
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

The function `publishCompressBytecode` takes in both the original `_bytecode` and the `_rawCompressedData`, the latter of which comes from the output of the server’s compression algorithm. Looping over the encoded data derived from `_rawCompressedData`, the corresponding chunks are pulled from the dictionary and compared to the original bytecode, reverting if there is a mismatch. After the encoded data has been verified, it is published to L1 and marked accordingly within the `KnownCodesStorage` contract.

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

ZKsync is a state-diff–based rollup and so publishing the correct state diffs plays an integral role in ensuring data availability.

### Difference between initial and repeated writes

ZKsync publishes state changes that happened within the batch instead of transactions themselves. Meaning that, for instance, some storage slot `S` under account `A` has changed to value `V`, we could publish a triple of `A,S,V`. Users, by observing all the triples, could restore the state of ZKsync. However, note that our tree, unlike Ethereum’s one, is not account based (i.e. there is no first layer of depth 160 of the Merkle tree corresponding to accounts and a second layer of depth 256 of the Merkle tree corresponding to users). Our tree is “flat,” i.e. a slot `S` under account `A` is just stored in the leaf number `H(S,A)`. Our tree is of depth 256 + 8 (the 256 is for these hashed account/key pairs and 8 is for potential shards in the future; we currently have only one shard and it is irrelevant for the rest of the document).

We call this `H(S,A)` a _derived key_, because it is derived from the address and the actual key in the storage of the account. Since our tree is flat, whenever a change happens, we can publish a pair `DK, V`, where `DK=H(S,A)`.

However, there is an optimization that can be done:

- Whenever a change to a key is used for the first time, we publish a pair of `DK,V` and we assign some sequential id to this derived key. This is called an _initial write_. It happens for the first time and that’s why we must publish the full key.  
- If this storage slot is published in some of the subsequent batches, instead of publishing the whole `DK`, we can use the sequential id instead. This is called a _repeated write_.  

For instance, if the slots `A`, `B` (I’ll use Latin letters instead of 32-byte hashes for readability) changed their values to `12`, `13` accordingly in the batch, they will be published in the following format:

- `(A, 12), (B, 13)`. Let’s say that the last sequential id ever used is 6. Then, `A` will receive the id of 7 and `B` will receive the id of 8.

Let’s say that in the next block, they change their values to `13`, `14`. Then, their diff will be published in the following format:

- `(7, 13), (8, 14)`.

The id is permanently assigned to each storage key that was ever published. While in the description above it may not seem like a huge boost, each `DK` is 32 bytes long and the id is at most 8 bytes long.

We call this id _enumeration_index_.

Note that the enumeration indexes are assigned in the order of a sorted array of (address, key) pairs, i.e. they are internally sorted. The enumeration indexes are part of the state Merkle tree; it is **crucial** that the initial writes are published in the correct order, so that anyone could restore the correct enumeration indexes for the storage slots. In addition, an enumeration index of `0` indicates that the storage write is an initial write.

### State diffs structure

Firstly, let’s define what we mean by _state diffs_. A _state diff_ is an element of the following structure.

[State diff structure](https://github.com/matter-labs/zksync-protocol/blob/main/crates/circuit_encodings/src/state_diff_record.rs#L8).

Basically, it contains all the values which might interest us about the state diff:

- `address` where the storage has been changed.  
- `key` (the original key inside the address).  
- `derived_key` — `H(key, address)` as described in the previous section.  
  - Note: the hashing algorithm currently used here is `Blake2s`.  
- `enumeration_index` — enumeration index as explained above. It is equal to 0 if the write is initial and contains the non-zero enumeration index if it is a repeated write (indexes start from 1).  
- `initial_value` — the value that was present in the key at the start of the batch.  
- `final_value` — the value that the key has changed to by the end of the batch.  

We will consider `stateDiffs` an array of such objects, sorted by (address, key).

This is the internal structure that is used by the circuits to represent the state diffs. The most basic “compression” algorithm is the one described above:

- For initial writes, write the pair (`derived_key`, `final_value`).  
- For repeated writes, write the pair (`enumeration_index`, `final_value`).  

Note that values like `initial_value`, `address`, and `key` are not used in the "simplified" algorithm above, but they will be helpful for more advanced compression algorithms in the future. The [algorithm](#state-diff-compression-format) for Boojum already utilizes the difference between the `initial_value` and `final_value` to save on pubdata.

### How the new pubdata verification works

#### **L2**

1. The operator provides both full `stateDiffs` (i.e. the array of the structs above) and the compressed state diffs (i.e. the array containing the state diffs, compressed by the algorithm explained [below](#state-diff-compression-format)).  
2. The `L2DAValidator` library must verify that the compressed version is consistent with the original stateDiffs and send the _hash_ of the `stateDiffs` to its L1 counterpart. It will also include the compressed state diffs in the totalPubdata to be published onto L1.  

#### **L1**

1. During block commitment, the standard DA protocol follows and the `L1DAValidator` is responsible for checking that the operator has provided the preimage for the `_totalPubdata`. More on how this is checked can be seen [here](./rollup_da.md).  
2. The block commitment [includes](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/state-transition/chain-deps/facets/Executor.sol#L550) _the hash of the `stateDiffs`_. Thus, ZKP verification will fail if the provided stateDiffs hash is not correct.  

It is a secure construction because the proof can be verified only if both the execution was correct and the hash of the `stateDiffs` is correct. This means that the `L2DAValidator` library indeed received the array of correct `stateDiffs` and, assuming the `L2DAValidator` is working correctly, double-checked that the compression is in the correct format, while L1 contracts at the commit stage double-checked that the operator provided the preimage for the compressed state diffs.

### State diff compression format

The following algorithm is used for the state diff compression:

[State diff compression v1 spec](./state_diff_compression_v1_spec.md)

## General pubdata format

The `totalPubdata` has the following structure:

1. First 4 bytes — the number of user L2→L1 logs in the batch.  
2. Then, the concatenation of packed L2→L1 user logs.  
3. Next, 4 bytes — the number of long L2→L1 messages in the batch.  
4. Then, the concatenation of L2→L1 messages, each in the format `<4 byte length || actual_message>`.  
5. Next, 4 bytes — the number of uncompressed bytecodes in the batch.  
6. Then, the concatenation of uncompressed bytecodes, each in the format `<4 byte length || actual_bytecode>`.  
7. Next, 4 bytes — the length of the compressed state diffs.  
8. Then, state diffs are compressed by the spec [above](#state-diff-compression-format).

The interface for committing batches is the following:

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
/// @param systemLogs Concatenation of all L2→L1 system logs in the batch
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
