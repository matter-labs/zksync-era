# RocksDB Wrapper

This crate adds the generic support for RocksDB storage. It provides more typesafe access to RocksDB and adds some
generic metrics (e.g., the size of merged batches, and the current file / memory sizes for each column family in the
database).

RocksDB is currently used in the [state keeper](../zksync_core/src/state_keeper) to speed up access to VM state, and by
the [Merkle tree](../merkle_tree).
