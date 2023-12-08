# State diff Compression
[Back to ToC](../../../README.md)

The most basic strategy to publish state diffs is to publish those in either of the following two forms:

- When a key is updated for the first time — `<key, value>`, where key is 32-byte derived key and the value is new
  32-byte value of the slot.
- When a key is updated for the second time and more — `<enumeration_index, value>`, where the `enumeration_index` is an
  8-byte id of the slot and the value is the new 32-byte value of the slot.

This compression strategy will utilize a similar idea for treating keys and values separately and it will be focused on
the efficient compression of keys and values separately.

## Keys

Keys will be packed in the same way as they were before. The only change is that we’ll avoid using the 8-byte
enumeration index and will pack it to the minimal necessary number of bytes. This number will be part of the pubdata.
Once a key has been used, it can already use the 4 or 5 byte enumeration index and it is very hard to have something
cheaper for keys that has been used already. The opportunity comes when remembering the ids for accounts to spare some
bytes on nonce/balance key, but ultimately the complexity may not be worth it.

There is some room for optimization of the keys that are being written for the first time, however, optimizing those is
more complex and achieves only a one-time effect (when the key is published for the first time), so they may be in scope
of the future upgrades.

## Values

Values are much easier to compress since they usually contain only zeroes. Also, we can leverage the nature of how those
values are changed. For instance, if nonce has been increased only by 1, we do not need to write the entire 32-byte new
value, we can just tell that the slot has been _increased_ and then supply only the 1-byte value by which it was
increased. This way instead of 32 bytes we need to publish only 2 bytes: first byte to denote which operation has been
applied and the second by to denote the number by which the addition has been made.

We have the following 4 types of changes: `Add`, `Sub,` `Transform`, `NoCompression` where:

- `NoCompression` denotes that the whole 32 byte will be provided.
- `Add` denotes that the value has been increased. (modulo 2^256)
- `Sub` denotes that the value has been decreased. (modulo 2^256)
- `Transform` denotes the value just has been changed (i.e. we disregard any potential relation between the previous and
  the new value, though the new value might be small enough to save up on the number of bytes).

Where the byte size of the output can be anywhere from 0 to 31 (also 0 makes sense for `Transform`, since it denotes
that it has been zeroed out). For `NoCompression` the whole 32 byte value is used.

So the format of the pubdata is the following:

**Part 1. Header.**

- `<version = 1 byte>` — this will enable easier automated unpacking in the future. Currently, it will be only equal to
  `1`.
- `<total_logs_len = 3 bytes>` — we need only 3 bytes to describe the total length of the L2→L1 logs.
- `<the number of bytes used for derived keys = 1 byte>`. It should be equal to the minimal required bytes to represent
  the enum indexes for repeated writes.

**Part 2. Initial writes.**

- `<num_of_initial_writes = 2 bytes>` - the number of initial writes. Since each initial write publishes at least 32
  bytes for key, then `2^16 * 32 = 2097152` will be enough for a lot of time (right now with the limit of 120kb it will
  take more than 15 L1 txs to use up all the space there).
- Then for each `<key, value>` pair for each initial write:
  - print key as 32-byte derived key.
  - packing type as a 1 byte value, which consists of 5 bits to denote the length of the packing and 3 bits to denote
    the type of the packing (either `Add`, `Sub`, `Transform` or `NoCompression`).
  - The packed value itself.

**Part 3. Repeated writes.**

Note, that there is no need to write the number of repeated writes, since we know that until the end of the pubdata, all
the writes will be repeated ones.

- For each `<key, value>` pair for each repeated write:
  - print key as derived key by using the number of bytes provided in the header.
  - packing type as a 1 byte value, which consists of 5 bits to denote the length of the packing and 3 bits to denote
    the type of the packing (either `Add`, `Sub`, `Transform` or `NoCompression`).
  - The packed value itself.

## Impact

This setup allows us to achieve nearly 75% packing for values, and 50% gains overall in terms of the storage logs based
on historical data.

## Encoding of packing type

Since we have `32 * 3 + 1` ways to pack a state diff, we need at least 7 bits to present the packing type. To make
parsing easier, we will use 8 bits, i.e. 1 byte.

We will use the first 5 bits to represent the length of the bytes (from 0 to 31 inclusive) to be used. The other 3 bits
will be used to represent the type of the packing: `Add`, `Sub` , `Transform`, `NoCompression`.

## Worst case scenario

The worst case scenario for such packing is when we have to pack a completely random new value, i.e. it will take us 32
bytes to pack + 1 byte to denote which type it is. However, for such a write the user will anyway pay at least for 32
bytes. Adding an additional byte is roughly 3% increase, which will likely be barely felt by users, most of which use
storage slots for balances, etc, which will consume only 7-9 bytes for packed value.

## Why do we need to repeat the same packing method id

You might have noticed that for each pair `<key, value>` to describe value we always first write the packing type and
then write the packed value. However, the reader might ask, it is more efficient to just supply the packing id once and
then list all the pairs `<key, value>` which use such packing.

I.e. instead of listing

(key = 0, type = 1, value = 1), (key = 1, type = 1, value = 3), (key = 2, type = 1, value = 4), …

Just write:

type = 1, (key = 0, value = 1), (key = 1, value = 3), (key = 2, value = 4), …

There are two reasons for it:

- A minor reason: sometimes it is less efficient in case the packing is used for very few slots (since for correct
  unpacking we need to provide the number of slots for each packing type).
- A fundamental reason: currently enum indeces are stored directly in the merkle tree & have very strict order of
  incrementing enforced by the circuits and (they are given in order by pairs `(address, key)`), which are generally not
  accessible from pubdata.

All this means that we are not allowed to change the order of “first writes” above, so indexes for them are directly
recoverable from their order, and so we can not permute them. If we were to reorder keys without supplying the new
enumeration indeces for them, the state would be unrecoverable. Always supplying the new enum index may add additional 5
bytes for each key, which might negate the compression benefits in a lot of cases. Even if the compression will still be
beneficial, the added complexity may not be worth it.

That being said, we _could_ rearange those for _repeated_ writes, but for now we stick to the same value compression
format for simplicity.

# Bytecode Compression doc 2

## Overview

As we are a rollup - all the bytecodes that contracts used in our chain must be copied into L1 (so that the chain can be
reconstructed from L1 if needed).

Given the want/need to cutdown on space used, bytecode is compressed prior to being posted to L1. At a high level
bytecode is chunked into opcodes (which have a size of 8 bytes), assigned a 2 byte index, and the newly formed byte
sequence (indexes) are verified and sent to L1. This process is split into 2 different parts: (1)
[the server side operator](https://github.com/matter-labs/zksync-era/blob/main/core/lib/utils/src/bytecode.rs#L31)
handling the compression and (2)
[the system contract](https://github.com/matter-labs/era-system-contracts/blob/main/contracts/BytecodeCompressor.sol)
verifying that the compression is correct before sending to L1.

## Example

Original bytecode

```
000000000000000A 000000000000000D 000000000000000A 000000000000000C
000000000000000B 000000000000000B 000000000000000D 000000000000000A
```

Dictionary would be:

```
0 -> 0xA (count: 3)
1 -> 0xD (count: 2, first seen: 1)
2 -> 0xB (count: 2, first seen: 4)
3 -> 0xC (count: 1)
```

Note that '1' maps to '0xD', as it occurs twice, and first occurrence is earlier than first occurence of 0xB, that also
occurs twice.

Compressed bytecode:

```
0008 0000 000000000000000A 000000000000000D 000000000000000B 000000000000000C

0000 0001 0000 0003 0002 0002 0001 0000
```

## Server Side Operator

This is the part that is responsible for taking bytecode, that has already been chunked into 8 byte words, performing
validation, and compressing it.

### Validation Rules

For bytecode to be considered valid it must satisfy the following:

1. Bytecode length must be less than 2097120 ((2^16 - 1) \* 32) bytes.
2. Bytecode length must be a multiple of 32.
3. Number of words cannot be even.

[Source](https://github.com/matter-labs/zksync-era/blob/main/core/lib/utils/src/bytecode.rs#L133)

### Compression Algorithm

At a high level, each 8 byte word from the chunked bytecode is assigned a 2 byte index (constraint on size of dictionary
of chunk → index is 2^16 + 1 elements). The length of the dictionary, dictionary entries (index assumed through order),
and indexes are all concatenated together to yield the final compressed version.

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

statistic.sort(primary=count, secondary=first_pos, order=desc)

for chunk in sorted_statistic:
    dictionary[chunk] = len(dictionary) # length of dictionary used to keep track of index

for chunk in chunked_bytecode:
    encoded_data.append(dictionary[chunk])

return [len(dictionary), dictionary.keys(order=index asc), encoded_data]
```

## System Contract Compression Verification & Publishing

The
[Bytecode Compressor](https://github.com/matter-labs/era-system-contracts/blob/main/contracts/BytecodeCompressor.sol)
contract performs validation on the compressed bytecode generated on the server side. At the current moment, publishing
bytecode to L1 may only be called by the bootloader but in the future anyone will be able to publish compressed bytecode
with no change to the underlying algorithm.

### Verification & Publication

The function `publishCompressBytecode` takes in both the original `_bytecode` and the `_rawCompressedData` , the latter
of which comes from the server’s compression algorithm output. Looping over the encoded data, derived from
`_rawCompressedData` , the corresponding chunks are retrieved from the dictionary and compared to the original byte
code, reverting if there is a mismatch. After the encoded data has been verified, it is published to L1 and marked
accordingly within the `KnownCodesStorage` contract.

Pseudo-code implementation:

```python
length_of_dict = _rawCompressedData[:2]
dictionary = _rawCompressedData[2:2 + length_of_dict * 8] # need to offset by bytes used to store length (2) and multiply by 8 for chunk size
encoded_data = _rawCompressedData[2 + length_of_dict * 8:]

assert(len(dictionary) % 8 == 0) # each element should be 8 bytes
assert(num_entries(dictionary) <= 2^16)
assert(len(encoded_data) * 4 == len(_bytecode)) # given that each chunk is 8 bytes and each index is 2 bytes they should differ by a factor of 4

for index in encoded_data:
    encoded_chunk = dictionary[index]
    real_chunk = _bytecode.readUint64(index * 4) # need to pull from index * 4 to account for difference in element size
    verify(encoded_chunk == real_chunk)

sendToL1(_rawCompressedBytecode)
markPublished(hash(_bytecode), hash(_rawCompressedData), len(_rawCompressedData))
```
