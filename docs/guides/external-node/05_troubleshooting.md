# ZKsync node Troubleshooting

The ZKsync node tries to follow the fail-fast principle: if an anomaly is discovered, instead of attempting state
recovery, in most cases it will restart. Most of the time it will manifest as crashes, and if it happens once, it
shouldn't be treated as a problem.

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

## Genesis Issues

The ZKsync node is supposed to start with an applied DB dump. If you see any genesis-related errors, it probably means
the ZKsync node was started without an applied dump.

[contact_us]: https://zksync.io/contact

## Logs

_Note: logs with the `error` level are reported to Sentry if it's configured. If you notice unneeded alerts there that
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
| WARN  | "Reorg detected"                                      | Reorg was detected on the main node, the ZKsync node will rollback and restart                           |

Same as with panics, normally it's only a problem if a WARN+ level log appears many times in a row.

## Metrics anomalies

The following common anomalies can be discovered by observing metrics _after the tree is rebuilt to match the DB
snapshot_:

- `external_node_sync_lag` doesn't decrease and `external_node_action_queue_action_queue_size` is near 0. Cause: The
  fetcher can't fetch new blocks quickly enough. Most likely, the network connection is too slow.
- `external_node_sync_lag` doesn't decrease and `external_node_action_queue_action_queue_size` is at some high level.
  Cause: The State Keeper doesn't process fetched data quickly enough. Most likely, a more powerful CPU is needed.
- `sql_connection_acquire` skyrockets. Probably, there are not enough connections in the pool to match the demand.
