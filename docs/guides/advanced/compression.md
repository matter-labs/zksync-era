# Bytecode Compression

## Overview

As we are a rollup - all the bytecodes that contracts used in our chain must be copied into L1 (so that the chain can be
reconstructed from L1 if needed).

Given the want/need to cutdown on space used, bytecode is compressed prior to being posted to L1. At a high level
bytecode is chunked into opcodes (which have a size of 8 bytes), assigned a 2 byte index, and the newly formed byte
sequence (indexes) are verified and sent to L1. This process is split into 2 different parts: (1)
[the server side operator](https://github.com/matter-labs/zksync-era/blob/main/core/lib/utils/src/bytecode.rs#L31)
handling the compression and (2)
[the system contract](https://github.com/matter-labs/era-system-contracts/blob/main/contracts/Compressor.sol) verifying
that the compression is correct before sending to L1.

## Example

Original bytecode:

```
0x000000000000000A000000000000000D000000000000000A000000000000000C000000000000000B000000000000000A000000000000000D000000000000000A000000000000000D000000000000000A000000000000000B000000000000000B
```

Split to 8-byte chunks:

```
000000000000000A 000000000000000D 000000000000000A 000000000000000C
000000000000000B 000000000000000A 000000000000000D 000000000000000A
000000000000000D 000000000000000A 000000000000000B 000000000000000B
```

Dictionary would be:

```
0 -> 0xA (count: 5)
1 -> 0xD (count: 3, first seen: 1)
2 -> 0xB (count: 3, first seen: 4)
3 -> 0xC (count: 1)
```

Note that `1` maps to `0xD`, as it occurs three times, and first occurrence is earlier than first occurrence of `0xB`,
that also occurs three times.

Compressed bytecode:

```
0x0004000000000000000A000000000000000D000000000000000B000000000000000C000000010000000300020000000100000001000000020002
```

Split into three parts:

1. `length_of_dict` is stored in the first 2 bytes
2. dictionary entries are stored in the next `8 * length_of_dict` bytes
3. 2-byte indices are stored in the rest of the bytes

```
0004

000000000000000A 000000000000000D 000000000000000B 000000000000000C

0000 0001 0000 0003
0002 0000 0001 0000
0001 0000 0002 0002
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

The [Bytecode Compressor](https://github.com/matter-labs/era-system-contracts/blob/main/contracts/Compressor.sol)
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
