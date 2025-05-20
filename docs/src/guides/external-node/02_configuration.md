# Node Configuration

This document outlines various configuration options for the EN. Currently, the Node requires the definition of numerous
environment variables. To streamline this process, we provide prepared configs for the ZKsync Era - for both
[mainnet](prepared_configs/mainnet-config.env) and [testnet](prepared_configs/testnet-sepolia-config.env). You can use
these files as a starting point and modify only the necessary sections.

**You can also see directory docker-compose-examples if you want to run external-node on your machine with recommended
default settings.**

## Database

The Node uses two databases: PostgreSQL and RocksDB.

PostgreSQL serves as the main source of truth in the EN, so all the API requests fetch the state from there. The
PostgreSQL connection is configured by the `DATABASE_URL`. Additionally, the `DATABASE_POOL_SIZE` variable defines the
size of the connection pool.

RocksDB is used in components where IO is a bottleneck, such as the State Keeper and the Merkle tree. If possible, it is
recommended to use an NVME SSD for RocksDB. RocksDB requires two variables to be set: `EN_STATE_CACHE_PATH` and
`EN_MERKLE_TREE_PATH`, which must point to different directories. When running a Node inside Docker Compose, Kubernetes
etc., these paths should point to dirs in a persistent volume (or 2 separate volumes). Persistent volumes **must** be
exclusive to a node; i.e., they **must not** be shared among nodes concurrently or transferred from one node to another.
Failing to adhere to this rule may result in RocksDB corruption or
[the Node crashing on start](05_troubleshooting.md#rocksdb-issues).

## L1 Web3 client

Node requires a connection to an Ethereum node. The corresponding env variable is `EN_ETH_CLIENT_URL`. Make sure to set
the URL corresponding to the correct L1 network (L1 mainnet for L2 mainnet and L1 sepolia for L2 testnet).

Note: Currently, the Node makes 2 requests to the L1 per L1 batch, so the Web3 client usage for a synced node should not
be high. However, during the synchronization phase the new batches would be persisted on the Node quickly, so make sure
that the L1 client won't exceed any limits (e.g. in case you use Infura).

## Exposed ports

The dockerized version of the server exposes the following ports:

- HTTP JSON-RPC: 3060
- WebSocket JSON-RPC: 3061
- Prometheus listener: 3322
- Healthcheck server: 3081

While the configuration variables for them exist, you are not expected to change them unless you want to use the Node
outside provided Docker environment (not supported at the time of writing).

## API limits

There are variables that allow you to fine-tune the limits of the RPC servers, such as limits on the number of returned
entries or the limit for the accepted transaction size. Provided files contain sane defaults that are recommended for
use, but these can be edited, e.g. to make the Node more/less restrictive.

**Some common API limits config:**

- `EN_MAX_RESPONSE_BODY_SIZE_MB` (default is 10 i.e. 10MB) controls max size of a single response. Hitting the limit
  will result in errors similar to: "Response is too big (...)".
- `EN_MAX_RESPONSE_BODY_SIZE_OVERRIDES_MB` overrides max response size for specific RPC methods. E.g., setting this var
  to `eth_getLogs=100,eth_getBlockReceipts=None` sets max response size for `eth_getLogs` to 100MB and disables size
  limiting for `eth_getBlockReceipts`, while other RPC methods will use the `EN_MAX_RESPONSE_BODY_SIZE_MB` setting.
- `EN_REQ_ENTITIES_LIMIT` (default 10,000) controls max possible limit of entities to be requested at once. Hitting the
  limit will result in errors similar to: "Query returned more than 10000 results (...)"

## JSON-RPC API namespaces

There are 7 total supported API namespaces: `eth`, `net`, `web3`, `debug` - standard ones; `zks` - rollup-specific one;
`pubsub` - a.k.a. `eth_subscribe`; `en` - used by Nodes while syncing. You can configure what namespaces you want to
enable using `EN_API_NAMESPACES` and specifying namespace names in a comma-separated list. By default, all but the
`debug` namespace are enabled.

## Optimizing RAM consumption

With the default config options, a node may consume large amounts of RAM (order of 32–64 GB for larger networks). This
consumption can be reduced with a moderate performance tradeoff as follows.

For large networks in terms of the state (both the current state size and history), the main component consuming RAM is
the Merkle tree (specifically, its RocksDB). Thus, enabling [pruning](08_pruning.md) and/or using
[snapshot recovery](07_snapshots_recovery.md) is an effective way to reduce RAM consumption. For example, an Era mainnet
node with [a 6-month pruning target](08_pruning.md#configuration) should consume about 10 GB RAM (vs ~60 GB for an
archive node).

For archive nodes, RAM consumption by the Merkle tree may be reduced by setting the following 2 options:

```shell
# Disables pinning indices and filters for the Merkle tree in RAM, which can occupy a large amount of it;
# e.g., they are ~30 GB on the Era mainnet for an archive node as of May 2025.
EN_MERKLE_TREE_INCLUDE_INDICES_AND_FILTERS_IN_BLOCK_CACHE=true
# **MUST** be used together with the previous option to set the RocksDB cache size. 4–8 GB provides a reasonable performance tradeoff
# on the Era mainnet as of May 2025. On smaller networks, acceptable values may be smaller.
EN_MERKLE_TREE_BLOCK_CACHE_SIZE_MB=4096
```

These options can be used together with pruning / snapshot recovery as well, but their _additional_ impact on RAM
consumption is expected to be fairly small.

## Logging and observability

- `MISC_LOG_FORMAT` defines the format in which logs are shown: `plain` corresponds to the human-readable format, while
  the other option is `json` (recommended for deployments).
- `RUST_LOG` variable allows you to set up the logs granularity (e.g. make the Node emit fewer logs). You can read about
  the format [here](https://docs.rs/env_logger/0.10.0/env_logger/#enabling-logging).
- `MISC_SENTRY_URL` and `MISC_OTLP_URL` variables can be configured to set up Sentry and OpenTelemetry exporters.
- If Sentry is configured, you also have to set `EN_SENTRY_ENVIRONMENT` variable to configure the environment in events
  reported to sentry.
