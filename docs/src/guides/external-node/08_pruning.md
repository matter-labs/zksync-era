# Pruning

It is possible to configure a Node to periodically prune all data from L1 batches older than a configurable threshold.
Data is pruned both from Postgres and from tree (RocksDB). Pruning happens continuously (i.e., does not require stopping
the node) in the background during normal node operation. It is designed to not significantly impact node performance.

Types of pruned data in Postgres include:

- Block and L1 batch headers
- Transactions
- EVM logs aka events
- Overwritten storage logs
- Transaction traces

Pruned data is no longer available via Web3 API of the node. The relevant Web3 methods, such as `eth_getBlockByNumber`,
will return an error mentioning the first retained block or L1 batch if queried pruned data.

## Interaction with snapshot recovery

Pruning and [snapshot recovery](07_snapshots_recovery.md) are independent features. Pruning works both for archival
nodes restored from a Postgres dump, and nodes recovered from a snapshot. Conversely, a node recovered from a snapshot
may have pruning disabled; this would mean that it retains all data starting from the snapshot indefinitely (but not
earlier data, see [snapshot recovery limitations](07_snapshots_recovery.md#current-limitations)).

A rough guide whether to choose the recovery option and/or pruning is as follows:

- If you need a node with data retention period of up to a few days, set up a node from a snapshot with pruning enabled
  and wait for it to have enough data.
- If you need a node with the entire rollup history, using a Postgres dump is the only option, and pruning should be
  disabled.
- If you need a node with significant data retention (order of months), the best option right now is using a Postgres
  dump. You may enable pruning for such a node, but beware that full pruning may take significant amount of time (order
  of weeks or months). In the future, we intend to offer pre-pruned Postgres dumps with a few months of data.

## Configuration

You can enable pruning by setting the environment variable

```yaml
EN_PRUNING_ENABLED: 'true'
```

By default, the node will keep L1 batch data for 7 days determined by the batch timestamp (always equal to the timestamp
of the first block in the batch). You can configure the retention period using:

```yaml
EN_PRUNING_DATA_RETENTION_SEC: '259200' # 3 days
```

The retention period can be set to any value, but for mainnet values under 24h will be ignored because a batch can only
be pruned after it has been executed on Ethereum.

Pruning can be disabled or enabled and the data retention period can be freely changed during the node lifetime.

```admonish warning
Pruning should be disabled when recovering the Merkle tree (e.g., if a node ran in
[the treeless mode](09_treeless_mode.md) before, or if its tree needs a reset for whatever reason). Otherwise, tree
recovery will with almost definitely result in an error, or worse, in a corrupted tree.
```

## Storage requirements for pruned nodes

The storage requirements depend on how long you configure to retain the data, but are roughly:

- **40GB + ~5GB/day of retained data** of disk space needed on machine that runs the node
- **300GB + ~15GB/day of retained data** of disk space for Postgres

```admonish note
When pruning an existing archival node, Postgres will be unable to reclaim disk space automatically. To reclaim disk
space, you need to manually run `VACUUM FULL`, which requires an `ACCESS EXCLUSIVE` lock. You can read more about it
in [Postgres docs](https://www.postgresql.org/docs/current/sql-vacuum.html).
```

## Monitoring pruning

Pruning information is logged with the following targets:

- **Postgres pruning:** `zksync_node_db_pruner`
- **Merkle tree pruning:** `zksync_metadata_calculator::pruning`, `zksync_merkle_tree::pruning`.

To check whether Postgres pruning works as intended, you should look for logs like this:

```text
2024-06-20T07:26:03.415382Z  INFO zksync_node_db_pruner: Soft pruned db l1_batches up to 8 and L2 blocks up to 29, operation took 14.850042ms
2024-06-20T07:26:04.433574Z  INFO zksync_node_db_pruner::metrics: Performed pruning of database, deleted 1 L1 batches, 2 L2 blocks, 68 storage logs, 383 events, 27 call traces, 12 L2-to-L1 logs
2024-06-20T07:26:04.436516Z  INFO zksync_node_db_pruner: Hard pruned db l1_batches up to 8 and L2 blocks up to 29, operation took 18.653083ms
```

(Obviously, timestamps and numbers in the logs will differ.)

Pruning logic also exports some metrics, the main of which are as follows:

| Metric name                                      | Type      | Labels       | Description                                         |
| ------------------------------------------------ | --------- | ------------ | --------------------------------------------------- |
| `db_pruner_not_pruned_l1_batches_count`          | Gauge     | -            | Number of retained L1 batches                       |
| `db_pruner_pruning_chunk_duration_seconds`       | Histogram | `prune_type` | Latency of a single pruning iteration               |
| `merkle_tree_pruning_deleted_stale_key_versions` | Gauge     | `bound`      | Versions (= L1 batches) pruned from the Merkle tree |
