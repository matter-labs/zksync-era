# Merkle Tree

Binary Merkle tree implementation based on amortized radix-16 Merkle tree (AR16MT) described in the [Jellyfish
Merkle tree] white paper. Unlike Jellyfish Merkle tree, our construction uses vanilla binary tree hashing algorithm to
make it easier for the circuit creation. The depth of the tree is 256, and Blake2 is used as the hashing function.

## Snapshot tests

In order to check backward compatibility of the tree implementation, it is snapshot-tested using the [`insta`] crate. If
any of snapshot tests fail, be sure to either fix your code, or update the snapshots being aware that the made changes
are probably not backward-compatible.

## Benchmarking

The `loadtest` example is a CLI app allowing to measure tree performance. It allows using the in-memory or RocksDB
storage backend, and Blake2 or no-op hashing functions. For example, the following command launches a benchmark with 75
blocks each containing 150,000 insertion operations.

```shell
cargo run --release -p zksync_merkle_tree --example loadtest_merkle_tree -- \
  --chunk-size=500 75 150000
```

The order of timings should be as follows (measured on MacBook Pro with 12-core Apple M2 Max CPU and 32 GB DDR5 RAM
using the command line above):

```text
Processing block #74
[metric] merkle_tree.load_nodes = 0.400870959 seconds
[metric] merkle_tree.extend_patch = 0.119743375 seconds
[metric] merkle_tree.extend_patch.new_leaves = 150000
[metric] merkle_tree.extend_patch.new_internal_nodes = 57588
[metric] merkle_tree.extend_patch.moved_leaves = 53976
[metric] merkle_tree.extend_patch.updated_leaves = 0
[metric] merkle_tree.extend_patch.avg_leaf_level = 26.74396987880927
[metric] merkle_tree.extend_patch.max_leaf_level = 44
[metric] merkle_tree.extend_patch.db_reads = 278133
[metric] merkle_tree.extend_patch.patch_reads = 96024
[metric] merkle_tree.finalize_patch = 0.707021 seconds
[metric] merkle_tree.leaf_count = 11250000
[metric] merkle_tree.finalize_patch.hashed_bytes = 3205548448 bytes
Processed block #74 in 1.228553208s, root hash = 0x1ddec3794d0a1c5b44c2d9c7aa985cc61c70e988da2e6f2a810e0eb37f4322c0
Committed block #74 in 571.588041ms
Verifying tree consistency...
Verified tree consistency in 37.478218666s
```

Full tree mode (with proofs) launched with the following command:

```shell
cargo run --release -p zksync_merkle_tree --example loadtest_merkle_tree -- \
  --chunk-size=500 --proofs --reads=50000 75 150000
```

...has the following order of timings:

```text
Processing block #74
[metric] merkle_tree.load_nodes = 0.5310345 seconds
[metric] merkle_tree.extend_patch = 0.905285834 seconds
[metric] merkle_tree.extend_patch.new_leaves = 150000
[metric] merkle_tree.extend_patch.new_internal_nodes = 57588
[metric] merkle_tree.extend_patch.moved_leaves = 53976
[metric] merkle_tree.extend_patch.updated_leaves = 0
[metric] merkle_tree.extend_patch.avg_leaf_level = 26.74396987880927
[metric] merkle_tree.extend_patch.max_leaf_level = 44
[metric] merkle_tree.extend_patch.key_reads = 50000
[metric] merkle_tree.extend_patch.db_reads = 400271
[metric] merkle_tree.extend_patch.patch_reads = 96024
[metric] merkle_tree.leaf_count = 11250000
[metric] merkle_tree.finalize_patch = 0.302226041 seconds
[metric] merkle_tree.finalize_patch.hashed_bytes = 3439057088 bytes
Processed block #74 in 1.814916125s, root hash = 0x1ddec3794d0a1c5b44c2d9c7aa985cc61c70e988da2e6f2a810e0eb37f4322c0
Committed block #74 in 904.560667ms
Verifying tree consistency...
Verified tree consistency in 37.935639292s
```

Launch the example with the `--help` flag for more details.

### Benchmarking pruning

`--prune` option enables tree pruning with some reasonable parameters and just the latest tree version retained. The
pruner should output `merkle_tree.pruning.*` metrics like this:

```text
[histogram] merkle_tree.pruning.load_stale_keys = 0.009145916 seconds
[histogram] rocksdb.write.batch_size{db=merkle_tree} = 649934 bytes
[gauge] rocksdb.live_data_size{db=merkle_tree, cf=default} = 1802196863 bytes
[gauge] rocksdb.total_sst_size{db=merkle_tree, cf=default} = 2057174959 bytes
[gauge] rocksdb.total_mem_table_size{db=merkle_tree, cf=default} = 67110912 bytes
[gauge] rocksdb.live_data_size{db=merkle_tree, cf=stale_keys} = 3275975 bytes
[gauge] rocksdb.total_sst_size{db=merkle_tree, cf=stale_keys} = 5141413 bytes
[gauge] rocksdb.total_mem_table_size{db=merkle_tree, cf=stale_keys} = 19924992 bytes
[histogram] merkle_tree.pruning.apply_patch = 0.031769875 seconds
[gauge] merkle_tree.pruning.target_retained_version = 2999
[histogram] merkle_tree.pruning.key_count = 48154
[gauge] merkle_tree.pruning.deleted_stale_key_versions{bound=start} = 2997
[gauge] merkle_tree.pruning.deleted_stale_key_versions{bound=end} = 3000
```

(at the end of the test in a setup with 3,000 blocks x 5,000 write ops / block). The same setup without pruning has the
following order of RocksDB storage consumption at the end of the test:

```text
[gauge] rocksdb.live_data_size{db=merkle_tree, cf=default} = 17723205116 bytes
[gauge] rocksdb.total_sst_size{db=merkle_tree, cf=default} = 17981011113 bytes
[gauge] rocksdb.total_mem_table_size{db=merkle_tree, cf=default} = 46139392 bytes
[gauge] rocksdb.live_data_size{db=merkle_tree, cf=stale_keys} = 441477770 bytes
[gauge] rocksdb.total_sst_size{db=merkle_tree, cf=stale_keys} = 441477770 bytes
[gauge] rocksdb.total_mem_table_size{db=merkle_tree, cf=stale_keys} = 19924992 bytes
```

I.e., pruning reduces RocksDB size approximately 8.7 times in this case.

[jellyfish merkle tree]: https://developers.diem.com/papers/jellyfish-merkle-tree/2021-01-14.pdf
[`insta`]: https://docs.rs/insta/
