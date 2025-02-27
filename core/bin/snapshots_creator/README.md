# Snapshots Creator

Snapshot creator is a command line tool for creating an app-level snapshot of the node storage at a certain L1 batch. A
snapshot does not contain full transaction history, but rather [a minimal subset of information](#snapshots-format)
needed to bootstrap an external node. A snapshot is always made at an L1 batch boundary.

Compared to Postgres dumps, app-level snapshots are much more compact; as of Feb 2024, the mainnet snapshot is approx.
13 GB (gzipped).

> **Warning.** Snapshot creator is in the early stage of development; expect bugs and incomplete / outdated docs. Use at
> your own risk.

## Local testing

Usage for local development (assuming the development environment [has been set up](../../../docs/guides/setup-dev.md)):

1. Run `zk env dev`
2. Generate storage logs, e.g. by running a load test with a command like
   `ACCOUNTS_AMOUNT=40 DURATION_SEC=60 zk run loadtest`.
3. Run the creator using `zk run snapshots-creator`.

To check that creating the snapshot succeeded, you may use the `snapshots` namespace of the JSON-RPC API of the main
node. With the local setup, the corresponding `curl` args are as follows:

```shell
curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc": "2.0", "id": 1, "method": "snapshots_getAllSnapshots", "params": [] }' \
  'http://localhost:3050'
# Should return an array of L1 batch numbers for all created snapshots, such as
# {
#   "snapshotsL1BatchNumbers": [42]
# }

curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc": "2.0", "id": 1, "method": "snapshots_getSnapshot", "params": [42] }' \
  'http://localhost:3050'
# Returns metadata for a specific snapshot containing `l1BatchNumber`, `miniblockNumber`
# and other fields.
```

By default, in the local setup snapshots are stored in the `artifacts/storage_logs_snapshots` directory relative to the
repository root. The storage location can be configured using the object store configuration to either use the local
filesystem, or Google Cloud Storage (GCS). Beware that for end-to-end testing of snapshot recovery, changes applied to
the main node configuration must be reflected in the external node configuration.

Creating a snapshot is a part of the [snapshot recovery integration test]. You can run the test using
`yarn recovery-test snapshot-recovery-test`. It requires the main node to be launched with a command like
`zk server --components api,tree,eth,state_keeper,commitment_generator`.

## Snapshots format

Each snapshot consists of three types of data (see [`snapshots.rs`] for exact definitions):

- **Header:** Includes basic information, such as the L2 block / L1 batch of the snapshot, L2 block / L1 batch
  timestamps, L2 block hash and L1 batch root hash. Returned by the methods in the `snapshots` namespace of the JSON-RPC
  API of the main node.
- **Storage log chunks:** Latest values for all VM storage slots ever written to at the time the snapshot is made.
  Besides key–value pairs, each storage log record also contains the L1 batch number of its initial write and its
  enumeration index; both are used to restore the contents of the `initial_writes` table. Chunking storage logs is
  motivated by their parallel generation; each chunk corresponds to a distinct non-overlapping range of hashed storage
  keys. (This should be considered an implementation detail for the purposes of snapshot recovery; recovery must not
  rely on any particular key distribution among chunks.) Stored as gzipped Protobuf messages in an [object store]; each
  chunk is a separate object.
- **Factory dependencies:** All bytecodes deployed on L2 at the time the snapshot is made. Stored as a single gzipped
  Protobuf message in an object store.

### Versioning

There are currently 2 versions of the snapshot format which differ in how keys are mentioned in storage logs.

- Version 0 includes key preimages (EVM-compatible keys), i.e. address / contract slot tuples.
- Version 1 includes only hashed keys as used in Era ZKP circuits and in the Merkle tree. Besides reducing the snapshot
  size (with the change, keys occupy 32 bytes instead of 52), this allows to unify snapshot recovery with recovery from
  L1 data. Having only hashed keys for snapshot storage logs is safe; key preimages are only required for a couple of
  components to sort keys in a batch, but these cases only require preimages for L1 batches locally executed on a node.

[`snapshots.rs`]: ../../lib/types/src/snapshots.rs
[object store]: ../../lib/object_store
[snapshot recovery integration test]: ../../tests/recovery-test/tests/snapshot-recovery.test.ts
