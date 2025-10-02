# Treeless Operation Mode

Normally, a Node needs to run the Merkle tree component (aka _metadata calculator_) in order to compute L1 batch state
root hashes. A state root hash from the previous batch can be accessed by L2 contracts, so processing transactions in an
L1 batch cannot start until the state root hash of the previous L1 batch is computed. Merkle tree requires non-trivial
storage space and RAM (roughly 3 TB and 32 GB respectively for an archival mainnet node as of July 2024). While storage
and RAM requirements can be significantly lowered with [snapshot recovery](07_snapshots_recovery.md) and
[pruning](08_pruning.md), **treeless operation mode** allows to run a node without a local Merkle tree instance at all.

## How it works

The relevant logic is encapsulated in the _tree fetcher_ component that can be run instead, or concurrently with the
Merkle tree component. Tree fetcher continuously loads L1 batch state root hashes for batches persisted to the node
storage. It uses the following 2 sources of hashes:

- The rollup contract on L1 (e.g. on the Ethereum mainnet for the Era mainnet). The state root hash for a batch is
  submitted to this contract as a part of the batch commitment.
- Main L2 node (or more generally, the L2 node that the current node is configured to sync from). Only used if the L1
  data source does not work (e.g., very recent L1 batches may be not yet committed to L1).

If the tree fetcher run concurrently to the Merkle tree, the tree will still compute state root hashes for all batches.
If the tree is slower than the fetcher (which is expected in most cases), it will compare the computed hash against the
state root hash from the tree fetcher and crash on a mismatch.

## Tradeoffs

- Tree fetcher requires limited trust to the L1 Web3 provider and the main L2 node (note that trust in them is required
  for other node components, such as [the consistency checker](06_components.md#consistency-checker) and
  [reorg detector](06_components.md#reorg-detector)). This trust is limited in time; mismatched L1 batch root hashes
  will eventually be detected by the 2 aforementioned components and the Merkle tree (if it is run concurrently).
- Tree fetcher only loads root hashes of the Merkle tree, not other tree data. That is, it cannot replace the Merkle
  tree if a node needs to serve the `zks_getProof` endpoint, since it fetches proofs from the Merkle tree.

## Configuration

The tree fetcher is disabled by default. You can enable it by specifying `tree_fetcher` in the list of components that a
node should run in the `--components` command-line arg. For example, to run all standard components and the tree
fetcher:

```shell
# Assume that the node binary in in $PATH
zksync_external_node --components=all,tree_fetcher
```

To run all standard components without the Merkle tree and the tree fetcher:

```shell
zksync_external_node --components=core,api,tree_fetcher
```

The tree fetcher currently does not have configurable parameters.

The tree fetcher can be freely switched on or off during the node lifetime; i.e., it's not required to commit to running
or not running it when initializing a node.

```admonish tip
Switching on the tree fetcher during [snapshot recovery](07_snapshots_recovery.md) can significantly speed it up
(order of 2–3 hours for the mainnet) because the node no longer needs to recover the Merkle tree before starting
catching up.
```

## Monitoring tree fetcher

Tree fetcher information is logged with the `zksync_node_sync::tree_data_fetcher` target.

Tree fetcher exports some metrics:

| Metric name                                                 | Type    | Labels   | Description                                                                                            |
| ----------------------------------------------------------- | ------- | -------- | ------------------------------------------------------------------------------------------------------ |
| `external_node_tree_data_fetcher_last_updated_batch_number` | Gauge   | -        | Last L1 batch with tree data updated by the fetcher                                                    |
| `external_node_tree_data_fetcher_step_outcomes`             | Counter | `kind`   | Number of times a fetcher step resulted in a certain outcome (e.g., update, no-op, or transient error) |
| `external_node_tree_data_fetcher_root_hash_sources`         | Counter | `source` | Number of root hashes fetched from a particular source (L1 or L2).                                     |
