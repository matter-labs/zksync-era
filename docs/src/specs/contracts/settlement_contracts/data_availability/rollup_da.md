# Rollup DA

[back to readme](../../README.md)

FIXME: run a spellchecker

## Prerequisites

Before reading this document, it is better to understand how [custom DA](./custom_da.md) in general works.

## EIP4844 support

EIP-4844, commonly known as Proto-Danksharding, is an upgrade to the ethereum protocol that introduces a new data availability solution embedded in layer 1. More information about it can be found [here](https://ethereum.org/en/roadmap/danksharding/).

To facilitate EIP4844 blob support, our circuits allow providing two arrays in our public input to the circuit:

- `blobCommitments` -- this is the commitment that helps to check the correctness of the blob content. The formula on how it is computed will be explained below in the document.
- `blobHash` -- the `keccak256` hash of the inner contents of the blob.

Note, that our circuits require that each blob contains exactly `4096 * 31` bytes. The maximal number of blobs that are supported by our proving system is 16, but the system contracts support only 6 blobs at most for now.

When committing a batch, the L1DAValidator is called with the data provided by the operator and it should return the two arrays described above. These arrays be put inside the batch commitment and then the correctness of the commitments will be verified at the proving stage.

Note, that the `Executor.sol` (and the contract itself) is not responsible for checking that the provided `blobHash` and `blobCommitments` in any way correspond to the pubdata inside the batch as it is the job of the DA Validator pair.

## Publishing pubdata to L1

Let's see an example of how the approach above works in rollup DA validators.

### RollupL2DAValidator

![RollupL2DAValidator.png](./img/Rollup_DA.png)

`RollupL2DAValidator` accepts the preimages for the data to publishes as well as their compressed format. After verifying the compression, it forms the `_totalPubdata` bytes array, which represents the entire blob of data that should be published to L1.

It calls the `PubdataChunkPublisher` system contract to split this pubdata into multiple "chunks" of size `4096 * 31` bytes and return the `keccak256` hash of those, These will be the `blobHash` of from the section before.

To give the flexibility of checking different DA, we send the following data to L1:

- State diff hash. As it will be used on L1 to confirm the correctness of the provided uncompressed storage diifs.
- The hash of the `_totalPubdata`. In case the size of pubdata is small, it will allow the operator also use just standard Ethereum calldata for the DA.
- Send the `blobHash` array.

### RollupL1DAValidator

When committing the batch, the operator will provide the preimage of the fields that the RollupL2DAValidator has sent before, and also some `l1DaInput` along with it. This `l1DaInput` will be used to prove that the pubdata was indeed provided in this batch.

The first byte of the `l1DaInput` denotes which way of pubdata publishing was used: Calldata or Blobs.

In case it is Calldata it will be just checked that the provided calldata matches the hash of the `_totalPubdata` that was sent by the L2 counterpart. Note, that Calldata may still contain the blob information as we typically start generating proves before we know which way of calldata will be used. Note, that in case the Calldata is used for DA, we do not verify the `blobCommitments` as the presence of the correct pubdata has been verified already.

In case it is blobs, we need to construct the `blobCommitment`s correctly for each of the blob of data.

For each of the `blob`s the operator provides so called `_commitment` that consists of the following packed structure: `opening point (16 bytes) || claimed value (32 bytes) || commitment (48 bytes) || proof (48 bytes)`.

The verification of the `_commitment` can be summarized in the following snippet:

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

The final `blobCommitment` is calculated as the hash between the `blobVersionedHash`, `opening point` and the `claimed value`. The zero knowledge circuits will verify that the opening point and the claimed value were calculated correctly and correspond to the data that was hashed under the `blobHash`.

## Structure of the pubdata

Rollups maintain the same structure of pubdata and apply the same rules for compresison as those that were used in the previous versions of the system. These can be read [here](./state_diff_compression_v1_spec.md).
