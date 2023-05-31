# ZKSync Storage

This crate adds the support for RocksDB storage - where we keep the information about the State and Merkle Tree.

## MerkleTree

This database is covering 2 column families:

- Tree
- LeafIndices

| Column      | Key                  | Value                          | Description |
| ----------- | -------------------- | ------------------------------ | ----------- |
| LeafIndices | 'leaf_index'         | u64 serialized                 |
| LeafIndices | tree leaf (32 bytes) | TODO: is it index of the tree? | TODO        |

## StateKeeper

This database has 3 columns:

- State
- Contracts
- FactoryDeps

| Column      | Key                             | Value                   | Description                          |
| ----------- | ------------------------------- | ----------------------- | ------------------------------------ |
| State       | 'block_number'                  | serialized block number | Last processed L1 batch number (u32) |
| State       | hash StorageKey (account + key) | 32 bytes value          | State for the given key              |
| Contracts   | address (20 bytes)              | `Vec<u8>`               | Contract contents                    |
| FactoryDeps | hash (32 bytes)                 | `Vec<u8>`               | TODO                                 |
