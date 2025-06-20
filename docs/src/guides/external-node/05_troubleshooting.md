# Node Troubleshooting

The Node tries to follow the fail-fast principle: if an anomaly is discovered, instead of attempting state recovery, in
most cases it will restart. Most of the time it will manifest as crashes, and if it happens once, it shouldn't be
treated as a problem.

However, if the node enters the crash loop or otherwise behaves unexpectedly, it may indicate either a bug in the
implementation or a problem with configuration. This section tries to cover common problems.

## Panics

Panics is the Rust programming language notion of irrecoverable errors, and normally if panic happens, the application
will immediately crash.

- Panic matching `called Result::unwrap() on an Err value: Database(PgDatabaseError`: problem communicating with the
  PostgreSQL, most likely some of the connections have died.
- Panic matching `failed to init rocksdb: Error { message: "IO error: No space left on device`: more space on SSD is
  required.
- Anything that mentions "Poison Error": a "secondary" panic that may occur if one of the components panicked first. If
  you see this panic, look for a panic that happened shortly before it to find the real cause.

Other kinds of panic aren't normally expected. While in most cases, the state will be recovered after a restart, please
[report][contact_us] such cases to Matter Labs regardless.

[contact_us]: https://zksync.io/contact

## Genesis Issues

On Era, a Node is supposed to start with an applied Postgres dump, or
[recover from a snapshot](07_snapshots_recovery.md) (the latter requires an opt-in by changing the node config; see the
linked article for details). If you see any genesis-related errors for an Era node without snapshot recovery activated,
it may mean the Node was started without an applied dump. For other networks, a Node may be able to sync from the
genesis.

## RocksDB Issues

When using Docker Compose, Kubernetes or other container orchestrators to run a Node, it is important to follow the
rules regarding persistent volumes for [RocksDB instances](02_configuration.md#database). Volumes must be attached to a
specific Node (effectively, to a specific Postgres database). Volumes **must not** be shared across nodes at the same
time (this may lead to RocksDB corruption), and **must not** be transferred from one node to another.

A symptom of incorrect persistent volume usage is a Node crashing on start with an error like this appearing in the
logs:

```text
ERROR zksync_node_framework::service: Task oneshot_runner failed: Oneshot task state_keeper/rocksdb_catchup_task failed
Caused by:
    0: Failed to catch up RocksDB to Postgres
    1: L1 batch number in state keeper cache (5153) is greater than the requested batch number (4652)
```

(Obviously, L1 batch numbers may differ.)

To fix the issue, allocate an exclusive persistent volume for the Node, ensure that the volume is empty and restart the
Node.

## Logs

_Note: logs with the `error` level are reported to Sentry if it is configured. If you notice unneeded alerts there that
you don't consider actionable, you may disable logs for a component by tweaking the configuration._

| Level | Log substring                                         | Interpretation                                                                                           |
| ----- | ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| ERROR | "One of the tokio actors unexpectedly finished"       | One of the components crashed, and the node is restarting.                                               |
| WARN  | "Stop signal received, <component> is shutting down"  | Satellite log of the message above                                                                       |
| ERROR | "A lot of requests to the remote API failed in a row" | The remote API used to update token lists is probably down. Logs should disappear once API is available. |
| WARN  | "Server returned an error status code: 429"           | The main API rate limits are too strict. [Contact][contact_us] Matter Labs to discuss the situation.     |
| WARN  | "Following transport error occurred"                  | There was a problem with fetching data from the main node.                                               |
| WARN  | "Unable to get the gas price"                         | There was a problem with fetching data from the main node.                                               |
| WARN  | "Consistency checker error"                           | There are problems querying L1, check the Web3 URL you specified in the config.                          |
| WARN  | "Reorg detected"                                      | Reorg was detected on the main node, the Node will rollback and restart                                  |

Same as with panics, normally it's only a problem if a WARN+ level log appears many times in a row.

## Metrics anomalies

The following common anomalies can be discovered by observing metrics _after the tree is rebuilt to match the DB
snapshot_:

- `external_node_sync_lag` doesn't decrease and `external_node_action_queue_action_queue_size` is near 0. Cause: The
  fetcher can't fetch new blocks quickly enough. Most likely, the network connection is too slow.
- `external_node_sync_lag` doesn't decrease and `external_node_action_queue_action_queue_size` is at some high level.
  Cause: The State Keeper doesn't process fetched data quickly enough. Most likely, a more powerful CPU is needed.
- `sql_connection_acquire` skyrockets. Probably, there are not enough connections in the pool to match the demand.

## Manual revert

Sometimes, it may be useful to revert the node state manually, as opposed to the automated revert performed by the
[reorg detector](06_components.md#reorg-detector). For example, if a node has missed a mandatory update, it may produce
block processing artifacts (e.g., events) diverging from the expected ones that are not caught by the reorg detector.

To perform a manual revert, ensure that the node is stopped (i.e., its Postgres and RocksDB instances are unused), and
execute the node binary with `revert $l1_batch_number` args. (If the node uses file-based configuration, the
corresponding args must be provided as well, as during the ordinary node run.) Here, `$l1_batch_number` is the number of
the last L1 batch to be retained by the node after the revert. The node will execute a revert and exit; after this, it
can be started normally.

```admonish warning
Do not revert the node state by manually removing rows from Postgres. The node uses RocksDB instances
(for [the Merkle tree](06_components.md#merkle-tree) and [state keeper](06_components.md#state-keeper--vm) cache)
besides Postgres, which are expected to be in sync with Postgres at all times.
If this invariant breaks, the RocksDB instances may become irreparably broken, which would require rebuilding the Merkle tree / state keeper cache
from scratch. This would result in significant performance degradation (for the cache) / delays in block processing (for the Merkle tree).
```
