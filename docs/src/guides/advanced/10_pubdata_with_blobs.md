# Pubdata Post 4844

## Motivation

EIP-4844, commonly known as Proto-Danksharding, is an upgrade to the ethereum protocol that introduces a new data
availability solution embedded in layer 1. More information about it can be found
[here](https://ethereum.org/en/roadmap/danksharding/). With proto-danksharding we can utilize the new blob data
availability for cheaper storage of pubdata when we commit batches resulting in more transactions per batch and cheaper
batches/transactions. We want to ensure we have the flexibility at the contract level to process both pubdata via
calldata, as well as pubdata via blobs. A quick callout here, while 4844 has introduced blobs as new DA layer, it is the
first step in full Danksharding. With full Danksharding ethereum will be able to handle a total of 64 blobs per block
unlike 4844 which supports just 6 per block.

> 💡 Given the nature of 4844 development from a solidity viewpoint, we’ve had to create a temporary contract
> `BlobVersionedHash.yul` which acts in place of the eventual `BLOBHASH` opcode.

## Technical Approach

The approach spans both L2 system contracts and L1 ZKsync contracts (namely `Executor.sol`). When a batch is sealed on
L2 we will chunk it into blob-sized pieces (4096 elements \* 31 bytes per what is required by our circuits), take the
hash of each chunk, and send them to L1 via system logs. Within `Executor.sol` , when we are dealing with blob-based
commitments, we verify that the blob contains the correct data with the point evaluation precompile. If the batch
utilizes calldata instead, the processing should remain the same as in a pre-4844 ZKsync. Regardless of if pubdata is in
calldata or blobs are used, the batch’s commitment changes as we include new data within the auxiliary output.

Given that this is the first step to a longer-term solution, and the restrictions of proto-danksharding that get lifted
for full danksharding, we impose the following constraints:

1. we will support a maximum of 2 blobs per batch
2. only 1 batch will be committed in a given transaction
3. we will always send 2 system logs (one for each potential blob commitment) even if the batch only uses 1 blob.

This simplifies the processing logic on L1 and stops us from increasing the blob base fee (increases when there 3 or
more blobs in a given block).

## Backward-compatibility

While some of the parameter formatting changes, we maintain the same function signature for `commitBatches` and still
allow for pubdata to be submitted via calldata:

```solidity
struct StoredBatchInfo {
  uint64 batchNumber;
  bytes32 batchHash;
  uint64 indexRepeatedStorageChanges;
  uint256 numberOfLayer1Txs;
  bytes32 priorityOperationsHash;
  bytes32 l2LogsTreeRoot;
  uint256 timestamp;
  bytes32 commitment;
}

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
  bytes pubdataCommitments;
}

function commitBatches(StoredBatchInfo calldata _lastCommittedBatchData, CommitBatchInfo[] calldata _newBatchesData)
  external;

```

## Implementation

### Bootloader Memory

With the increase in the amount of pubdata due to blobs, changes can be made to the bootloader memory to facilitate more
l2 to l1 logs, compressed bytecodes, and pubdata. We take the naive approach for l2 to l1 logs and the compressed
bytecode, doubling their previous constraints from `2048` logs and `32768 slots` to `4096 logs` and `65536 slots`
respectively. We then increase the number of slots for pubdata from `208000` to `411900`. Copying the comment around
pubdata slot calculation from our code:

```solidity
One of "worst case" scenarios for the number of state diffs in a batch is when 240kb of pubdata is spent
on repeated writes, that are all zeroed out. In this case, the number of diffs is 240k / 5 = 48k. This means that they will have
accommodate 13056000 bytes of calldata for the uncompressed state diffs. Adding 120k on top leaves us with
roughly 13176000 bytes needed for calldata. 411750 slots are needed to accommodate this amount of data.
We round up to 411900 slots just in case.
```

The overall bootloader max memory is increased from `24000000` to `30000000` bytes to accommodate the increases.

### L2 System Contracts

We introduce a new system contract PubdataChunkPublisher that takes the full pubdata, creates chunks that are each
126,976 bytes in length (this is calculated as 4096 elements per blob each of which has 31 bytes), and commits them in
the form of 2 system logs. We have the following keys for system logs:

```solidity
enum SystemLogKey {
  L2_TO_L1_LOGS_TREE_ROOT_KEY,
  TOTAL_L2_TO_L1_PUBDATA_KEY,
  STATE_DIFF_HASH_KEY,
  PACKED_BATCH_AND_L2_BLOCK_TIMESTAMP_KEY,
  PREV_BATCH_HASH_KEY,
  CHAINED_PRIORITY_TXN_HASH_KEY,
  NUMBER_OF_LAYER_1_TXS_KEY,
  BLOB_ONE_HASH_KEY,
  BLOB_TWO_HASH_KEY,
  EXPECTED_SYSTEM_CONTRACT_UPGRADE_TX_HASH_KEY
}

```

In addition to the blob commitments, the hash of the total pubdata is still sent and is used if a batch is committed
with pubdata as calldata vs as blob data. As stated earlier, even when we only have enough pubdata for a single blob, 2
system logs are sent. The hash value in the second log in this case will `bytes32(0)` .

One important thing is that we don’t try to reason about the data here, that is done in the L1Messenger and Compressor
contracts. The main purpose of this is to commit to blobs and have those commitments travel to L1 via system logs.

### L1 Executor Facet

While the function signature for `commitBatches` and the structure of `CommitBatchInfo` stays the same, the format of
`CommitBatchInfo.pubdataCommitments` changes. Before 4844, this field held a byte array of pubdata, now it can hold
either the total pubdata as before or it can hold a list of concatenated info for kzg blob commitments. To differentiate
between the two, a header byte is prepended to the byte array. At the moment we only support 2 values:

```solidity
/// @dev Enum used to determine the source of pubdata. At first we will support calldata and blobs but this can be extended.
enum PubdataSource {
    Calldata = 0,
    Blob = 1
}
```

We reject all other values in the first byte.

### Calldata Based Pubdata Processing

When using calldata, we want to operate on `pubdataCommitments[1:pubdataCommitments.length - 32]` as this is the full
pubdata that was committed to via system logs. The reason we don’t operate on the last 32 bytes is that we also include
what the blob commitment for this data would be as a way to make our witness generation more generic. Only a single blob
commitment is needed for this as the max size of calldata is the same size as a single blob. When processing the system
logs in this context, we will check the hash of the supplied pubdata without the 1 byte header for pubdata source
against the value in the corresponding system log with key `TOTAL_L2_TO_L1_PUBDATA_KEY`. We still require logs for the 2
blob commitments, even if these logs contain values we will substitute them for `bytes32(0)` when constructing the batch
commitment.

### Blob Based Pubdata Processing

The format for `pubdataCommitments` changes when we send pubdata as blobs, containing data we need to verify the blob
contents via the newly introduced point evaluation precompile. The data is `pubdataCommitments[1:]` is the concatenation
of `opening point (16 bytes) || claimed value (32 bytes) || commitment (48 bytes) || proof (48 bytes)` for each blob
attached to the transaction, lowering our calldata from N → 144 bytes per blob. More on how this is used later on.

Utilizing blobs causes us to process logs in a slightly different way. Similar to how it's done when pubdata is sent via
calldata, we require a system log with a key of the `TOTAL_L2_TO_L1_PUBDATA_KEY` , although the value is ignored and
extract the 2 blob hashes from the `BLOB_ONE_HASH_KEY` and `BLOB_TWO_HASH_KEY` system logs to be used in the batch
commitment.

While calldata verification is simple, comparing the hash of the supplied calldata versus the value in the system log,
we need to take a few extra steps when verifying the blobs attached to the transaction contain the correct data. After
processing the logs and getting the 2 blob linear hashes, we will have all the data we need to call the
[point evaluation precompile](https://eips.ethereum.org/EIPS/eip-4844#point-evaluation-precompile). Recall that the
contents of `pubdataCommitments` have the opening point (in its 16 byte form), claimed value, the commitment, and the
proof of this claimed value. The last piece of information we need is the blob’s versioned hash (obtained via `BLOBHASH`
opcode).

There are checks within `_verifyBlobInformation` that ensure that we have the correct blob linear hashes and that if we
aren’t expecting a second blob, the linear hash should be equal to `bytes32(0)`. This is how we signal to our circuits
that we didn’t publish any information in the second blob.

Verifying the commitment via the point evaluation precompile goes as follows (note that we assume the header byte for
pubdataSource has already been removed by this point):

```solidity
// The opening point is passed as 16 bytes as that is what our circuits expect and use when verifying the new batch commitment
// PUBDATA_COMMITMENT_SIZE = 144 bytes
pubdata_commitments <- [opening point (16 bytes) || claimed value (32 bytes) || commitment (48 bytes) || proof (48 bytes)] from calldata
opening_point = bytes32(pubdata_commitments[:16])
versioned_hash <- from BLOBHASH opcode

// Given that we needed to pad the opening point for the precompile, append the data after.
point_eval_input = versioned_hash || opening_point || pubdataCommitments[16: PUBDATA_COMMITMENT_SIZE]

// this part handles the following:
// verify versioned_hash == hash(commitment)
// verify P(z) = y
res <- point_valuation_precompile(point_eval_input)

assert uint256(res[32:]) == BLS_MODULUS
```

Where correctness is validated by checking the latter 32 bytes of output from the point evaluation call is equal to
`BLS_MODULUS`.

### Batch Commitment and Proof of Equivalence

With the contents of the blob being verified, we need to add this information to the batch commitment so that it can
further be part of the verification of the overall batch by our proof system. Our batch commitment is the hashing of a
few different values: passthrough data (holding our new state root, and next enumeration index to be used), meta
parameters (flag for if zk porter is available, bootloader bytecode hash, and default account bytecode hash), and
auxiliary output. The auxiliary output changes with 4844, adding in 4 new fields and the new corresponding encoding:

- 2 `bytes32` fields for linear hashes
  - These are the hashes of the blob’s preimages
- 2 `bytes32` for 4844 output commitment hashes
  - These are `(versioned hash || opening point || evaluation value)`
  - The format of the opening point here is expected to be the 16 byte value passed by calldata
- We encode an additional 28 `bytes32(0)` at the end because with the inclusion of vm 1.5.0, our circuits support a
  total of 16 blobs that will be used once the total number of blobs supported by ethereum increase.

```solidity
abi.encode(
    l2ToL1LogsHash,
    _stateDiffHash,
    _batch.bootloaderHeapInitialContentsHash,
    _batch.eventsQueueStateHash,
    _blob1LinearHash,
    _blob1OutputCommitment,
    _blob2LinearHash,
    _blob2OutputCommitment,
    _encode28Bytes32Zeroes()
);
```

There are 3 different scenarios that change the values posted here:

1. We submit pubdata via calldata
2. We only utilize a single blob
3. We use both blobs

When we use calldata, the values `_blob1LinearHash`, `_blob1OutputCommitment`, `_blob2LinearHash`, and
`_blob2OutputCommitment` should all be `bytes32(0)`. If we are using blobs but only have a single blob,
`_blob1LinearHash` and `_blob1OutputCommitment` should correspond to that blob, while `_blob2LinearHash` and
`_blob2OutputCommitment` will be `bytes32(0)`. Following this, when we use both blobs, the data for these should be
present in all of the values.

Our circuits will then handle the proof of equivalence, following a method similar to the moderate approach mentioned
[here](https://notes.ethereum.org/@vbuterin/proto_danksharding_faq#Moderate-approach-works-with-any-ZK-SNARK), verifying
that the total pubdata can be repackaged as the blobs we submitted and that the commitments in fact evaluate to the
given value at the computed opening point.

## Pubdata Contents and Blobs

Given how data representation changes on the consensus layer (where blobs live) versus on the execution layer (where
calldata is found), there is some preprocessing that takes place to make it compatible. When calldata is used for
pubdata, we keep it as is and no additional processing is required to transform it. Recalling the above section when
pubdata is sent via calldata it has the format: source byte (1 bytes) || pubdata || blob commitment (32 bytes) and so we
must first trim it of the source byte and blob commitment before decoding it. A more detailed guide on the format can be
found in our documentation. Using blobs requires a few more steps:

```python
ZKSYNC_BLOB_SIZE = 31 * 4096

# First we pad the pubdata with the required amount of zeroes to fill
# the nearest blobs
padding_amount = ZKSYNC_BLOB_SIZE - len(pubdata) % ZKSYNC_BLOB_SIZE)
padded_pubdata = pad_right_with_zeroes(pubdata, padding_amount)

# We then chunk them into `ZKSYNC_BLOB_SIZE` sized arrays
blobs = chunk(padded_pubdata, ZKSYNC_BLOB_SIZE)

# Each blob is then encoded to be compatible with the CL
for blob in blobs:
    encoded_blob = zksync_pubdata_into_ethereum_4844_data(blob)
```

Now we can apply the encoding formula, with some of the data from the blob commit transaction to move from encoded blobs
back into decodable zksync pubdata:

```python
# opening point (16 bytes) || claimed value (32 bytes) || commitment (48 bytes) || proof (48 bytes)
BLOB_PUBDATA_COMMITMENT_SIZE = 144

# Parse the kzg commitment from the commit calldata
commit_calldata_without_source = commit_calldata[1:]
for i in range(0, len(commit_calldata_without_source), BLOB_PUBDATA_COMMITMENT_SIZE):
    # We can skip the opening point and claimed value, ignoring the proof
    kzg_commitment = commit_calldata_without_source[48:96]

# We then need to pull the blobs in the correct order, this can be found by matching
# each blob with their kzg_commitment keeping the order from the calldata
encoded_blobs = pull_blob_for_each_kzg_commitment(kzg_commitments)

# Decode each blob into the zksync specific format
for encoded_blob in encoded_blobs:
    decoded_blob = ethereum_4844_data_into_zksync_pubdata(encoded_blob)

reconstructed_pubdata = concat(decoded_blobs)
```

The last thing to do depends on the strategy taken, the two approaches are:

- Remove all trailing zeroes after concatenation
- Parse the data and ignore the extra zeroes at the end

The second option is a bit messier so going with the first, we can then decode the pubdata and when we get to the last
state diff, if the number of bytes is less than specified we know that the remaining data are zeroes. The needed
functions can be found within the
[zkevm_circuits code](https://github.com/matter-labs/era-zkevm_circuits/blob/3a973afb3cf2b50b7138c1af61cc6ac3d7d0189f/src/eip_4844/mod.rs#L358).
