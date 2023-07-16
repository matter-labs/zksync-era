# In-memory Merkle tree

Simple in-memory binary Merkle tree implementation. The tree is of bounded depth (up to 1,024 leaves) and uses the
keccak-256 hash function.

## Benchmarking

The tree implementation comes with a `criterion` benchmark that can be run with a command like this:

```shell
cargo bench -p zksync_mini_merkle_tree --bench tree
```

The order of timings should be 2M elements/s for all tree sizes (measured on MacBook Pro with 12-core Apple M2 Max CPU),
both for calculating the root and the root + Merkle path. This translates to ~130µs for a tree with 512 leaves (the tree
size used for `L2ToL1Log`s).
