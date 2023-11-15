# Snapshots Creator

Snapshot creator is small command line tool for creating a snapshot of zkSync node for EN node to be able to initialize
to a certain L1 Batch.

Snapshots do not contain full transactions history, but rather a minimal subset of information needed to bootstrap EN
node.

Usage (local development):\
First run `zk env dev` \
then the creator can be run using:  
`cargo run --bin snapshots_creator --release`

Snapshot contents can be stored based on blob_store config either in local filesystem or GS.

## Snapshots format

Each snapshot consists of three types of objects: header, storage logs chunks and factory deps:

- Snapshot Header (currently returned by snapshots namespace of JSON-RPC API)

```rust
pub struct SnapshotHeader {
    pub l1_batch_number: L1BatchNumber,
    pub miniblock_number: MiniblockNumber,
    // ordered by chunk_id
    pub storage_logs_chunks: Vec<SnapshotStorageLogsChunkMetadata>,
    pub factory_deps_filepath: String,
    pub last_l1_batch_with_metadata: L1BatchWithMetadata,
    pub generated_at: DateTime<Utc>,
}

pub struct SnapshotStorageLogsChunkMetadata {
    pub chunk_id: u64,
    // can be either a gs or filesystem path
    pub filepath: String,
}
```

- Snapshot Storage logs chunks (most likely to be stored in gzipped protobuf files, but this part is still WIP) :

```rust
pub struct SnapshotStorageLogsChunk {
    // sorted by hashed_keys interpreted as little-endian numbers
    pub storage_logs: Vec<SnapshotStorageLog>,
}

// "most recent" for each key together with info when the key was first used
pub struct SnapshotStorageLog {
    pub key: StorageKey,
    pub value: StorageValue,
    pub l1_batch_number_of_initial_write: L1BatchNumber,
    pub enumeration_index: u64,
}
```

- Factory dependencies (most likely to be stored as protobufs in the very near future)

```rust
pub struct SnapshotFactoryDependencies {
		pub factory_deps: Vec<SnapshotFactoryDependency>
}

pub struct SnapshotFactoryDependency {
    pub bytecode_hash: H256,
    pub bytecode: Vec<u8>,
}
```
