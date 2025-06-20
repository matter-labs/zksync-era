# Node Configuration

This document outlines various configuration options for the EN. Currently, the Node reads configuration from
environment variables (generally prefixed with `EN_`), and/or from YAML files. To use YAML-based config params, it must
be specified by providing `--config-path` command-line argument provided to the node binary. If both env vars and YAML
are used, environment variables have higher priority.

In the docs, we use env variable names for config params (e.g., `EN_MERKLE_TREE_PATH`), or its dot-separated paths in
YAML like `database.merkle_tree.path`. A path always maps to an env variable obtained by replacing dots `.` with
underscores `_`, capitalizing and prepending the `EN_` prefix. For example, `db.merkle_tree.path` is transformed to
`EN_DB_MERKLE_TREE_PATH`. Some params have aliases or fallbacks (generally, non-prefixed env variables); these are
exhaustively documented [in the config help](#getting-help-on-configuration-params). For example, the
`EN_MERKLE_TREE_PATH` env var (or `merkle_tree.path`) is a deprecated alias for the aforementioned `db.merkle_tree.path`
param. If a deprecated alias is used, a warning is logged with the `smart_config` target. It is recommended (but not
immediately required) to convert config sources to use a canonical param path, which is output as a part of the warning.

```admonish tip
To output all config params in canonical formats / locations with node v${FIXME_VERSION}+, run the node binary
with `config print` command-line args, or `config print --diff` to only output non-default param valuess.
See the `config print --help` docs for more options.
```

To streamline the config setup, we provide prepared configs for ZKsync Era – for both
[mainnet](prepared_configs/mainnet-config.env) and [testnet](prepared_configs/testnet-sepolia-config.env). You can use
these files as a starting point and modify only the necessary sections.

```admonish tip
You can see [Docker Compose examples](docker-compose-examples) if you want to run external-node on your machine with recommended
default settings.
```

## Getting help on configuration params

```admonish note
The functionality described in this section is only present in releases v${FIXME_VERSION}+. It works identically for the main node
binary (`zksync_server`).
```

The node binary comes with a builtin help on all configuration parameters. This help can be triggered by launching the
node binary with the `config help` command-line args:

```shell
zksync_external_node config help
# ...or, if using zkstack:
zkstack external-node -- config help
```

By providing an argument after `help`, it's possible to filter params by name; e.g., `config help port` will output help
for all params containing "port" in their path.

## Parameter format

Formats for specific params are mentioned in their [help](#getting-help-on-configuration-params). One of the more
flexible formats are parameters with units (durations or byte sizes).

- The canonical format for such params is a string consisting of an integer number + the unit, e.g. `5sec`, `1 day`, or
  `100 MB`.
- Besides that, it can be an object with a single field corresponding to the unit, and an integer value. For example,
  `{ "sec": 5 }`. The unit may be prefixed with `in_`; e.g. `{ "in_sec": 5 }`.
- As a part of the general coercion rules, the object format can be flattened into the parent. E.g., instead of
  specifying `timeout: 5 sec`, one can use `timeout_sec: 5` or `timeout_in_sec: 5`. This applies to env variables; e.g.,
  the `db.merkle_tree.block_cache_size` param (a byte size) can be specified as either of
  `EN_DB_MERKLE_TREE_BLOCK_CACHE_SIZE=128 MB` or `EN_DB_MERKLE_TREE_BLOCK_CACHE_SIZE_MB=128`.

If it's necessary to pass a value with a complex structure (a JSON array or object) as an env variable, this can be
accomplished by suffixing the parameter with `__JSON`:

```shell
# Sets the `api.web3_json_rpc.api_namespaces` param
EN_API_WEB3_JSON_RPC_API_NAMESPACES__JSON='["zks", "eth"]'
```

## Debugging configuration

```admonish note
The functionality described in this section is only present in releases v${FIXME_VERSION}+. It works identically for the main node
binary (`zksync_server`).
```

The node provides info on params as `DEBUG` logs with the `zksync_config` target at the node start, like this:

```text
2025-06-04T09:17:21.615338Z DEBUG zksync_config::observability_ext: parsed config param
  param="http_port"
  path="api.web3_json_rpc.http_port"
  config="Web3JsonRpcConfig"
  origin=YAML file './chains/era/configs/general.yaml' -> path 'api.web3_json_rpc.http_port'
  value=3050
  is_secret=false
  is_default=true
```

Naturally, value for the secret params are not exposed; instead, `is_secret` is set to `true`. Some additional logging
happens inside the config library (i.e., with `smart_config::*` targets).

If configuration parsing fails, the node will print exhaustive failure reason(s) with rich context:

```text
Error: failed parsing config param(s): 3 error(s) in total
1. error parsing param `filters_limit` in `Web3JsonRpcConfig` at `api.web3_json_rpc.filters_limit`
  [origin: YAML file './chains/era/configs/general.yaml' -> path 'api.web3_json_rpc.filters_limit']:
  invalid value: integer `-10000`, expected usize
2. error parsing param `port` in `MerkleTreeApiConfig` at `api.merkle_tree.port`
  [origin: YAML file './chains/era/configs/general.yaml' -> path 'api.merkle_tree.port']:
  invalid digit found in string while parsing u16 value '3072?'
3. error parsing param `block_cache_size` in `MerkleTreeConfig` at `db.merkle_tree.block_cache_size`
  [origin: env variable 'EN_DB_MERKLE_TREE_BLOCK_CACHE_SIZE']:
  invalid type: string "what", expected value with unit, like '32 MB'
```

Other than logs, config params are exposed as INFO metrics named `server_config_params`:

```text
server_config_params{default="false",path="api.healthcheck.port",secret="false",value="3071"} 1
```

If the `api.healthcheck.expose_config` param is set, config params are also output as a part of the `config` component
in [the healthcheck endpoint](#exposed-ports). In this case, they can be accessed like this:

```shell
$ curl -s http://127.0.0.1:3081/health | jq '.components.config.details."api.merkle_tree.port"'
{
  "origin": "YAML file './chains/era/configs/general.yaml' -> path 'api.merkle_tree.port'",
  "value": 3072
}
```

There are a couple of node CLI commands for debugging as well:

- `config debug` outputs param values with debugging info (e.g., the value origin), optionally filtered by the param
  name.
- `config print` outputs the entire merged configuration in a single YAML file or env variables list.

See command help using `config debug --help` / `config print --help` for details.

```admonish tip
Besides debugging, `config print` is also useful to migrate all config params to their canonical locations / formats.
```

## Params overview

### Database

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

### L1 Web3 client

Node requires a connection to an Ethereum node. The corresponding env variable is `EN_ETH_CLIENT_URL`. Make sure to set
the URL corresponding to the correct L1 network (L1 mainnet for L2 mainnet and L1 sepolia for L2 testnet).

Note: Currently, the Node makes 2 requests to the L1 per L1 batch, so the Web3 client usage for a synced node should not
be high. However, during the synchronization phase the new batches would be persisted on the Node quickly, so make sure
that the L1 client won't exceed any limits (e.g. in case you use Infura).

### Exposed ports

The dockerized version of the server exposes the following ports:

- HTTP JSON-RPC: 3060
- WebSocket JSON-RPC: 3061
- Prometheus listener: 3322
- Healthcheck server: 3081

While the configuration variables for them exist, you are not expected to change them unless you want to use the Node
outside provided Docker environment (not supported at the time of writing).

### API limits

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

### JSON-RPC API namespaces

There are 7 total supported API namespaces: `eth`, `net`, `web3`, `debug` - standard ones; `zks` - rollup-specific one;
`pubsub` - a.k.a. `eth_subscribe`; `en` - used by Nodes while syncing. You can configure what namespaces you want to
enable using `EN_API_NAMESPACES` and specifying namespace names in a comma-separated list. By default, all but the
`debug` namespace are enabled.

### API caching

The API server performs in-memory caching of data used for VM invocations (i.e., `eth_call`, `eth_estimateGas`,
`eth_sendRawTransaction` methods), such as state values and bytecodes. Cache sizes have reasonable defaults, but can be
adjusted to tradeoff between performance and RAM consumption of the node.

- `EN_LATEST_VALUES_CACHE_SIZE_MB` is the size of the latest values cache, used for VM invocations against the latest
  block. This is likely to be most utilized of the caches, potentially with hundreds / thousands of hits per VM
  invocation.
- `EN_FACTORY_DEPS_CACHE_SIZE_MB` is the size of the bytecodes (aka factory deps) cache.
- `EN_INITIAL_WRITES_CACHE_SIZE_MB` is the size of the initial writes caches. Initial write info is used as a part of
  the gas pricing model.

### Optimizing RAM consumption

With the default config options, a node may consume large amounts of RAM (order of 32–64 GB for larger networks). This
consumption can be reduced with a moderate performance tradeoff as follows.

For large networks in terms of the state (both the current state size and history), the main component consuming RAM is
the Merkle tree (specifically, its RocksDB). Thus, enabling [pruning](08_pruning.md) and/or using
[snapshot recovery](07_snapshots_recovery.md) is an effective way to reduce RAM consumption. For example, an Era mainnet
node with [a 6-month pruning target](08_pruning.md#configuration) should consume about 10 GB RAM (vs ~60 GB for an
archive node).

For archive nodes, RAM consumption by the Merkle tree may be reduced by setting the following options:

```shell
# Disables pinning indices and filters for the Merkle tree in RAM, which can occupy a large amount of it;
# e.g., they are ~30 GB on the Era mainnet for an archive node as of May 2025.
EN_MERKLE_TREE_INCLUDE_INDICES_AND_FILTERS_IN_BLOCK_CACHE=true
# **MUST** be used together with the previous option to set the RocksDB cache size. 4–8 GB provides a reasonable performance tradeoff
# on the Era mainnet as of May 2025. On smaller networks, acceptable values may be smaller.
EN_MERKLE_TREE_BLOCK_CACHE_SIZE_MB=4096
# Limits the number of concurrently open files, which in turn influences OS-level page cache. A reasonable value
# heavily depends on the amount of data in the tree.
EN_MERKLE_TREE_MAX_OPEN_FILES=1024
```

These options can be used together with pruning / snapshot recovery as well, but their _additional_ impact on RAM
consumption is expected to be fairly small. The options can be changed or removed at any time; they do not require a
long-term commitment or irreversible decisions, unlike enabling pruning for a node.

### Logging and observability

- `EN_LOG_FORMAT` env var (deprecated alias `MISC_LOG_FORMAT`) defines the format in which logs are shown. `plain`
  corresponds to the human-readable format, while the other option is `json` (recommended for deployments).
- `EN_LOG_DIRECTIVES` env var (also read from `RUST_LOG`) allows you to set up the logs granularity (e.g. make the Node
  emit fewer logs). You can read about the format
  [here](https://docs.rs/env_logger/0.10.0/env_logger/#enabling-logging).
- `EN_SENTRY_URL` env var (deprecated alias `MISC_SENTRY_URL`) can be configured to set up a Sentry exporter.
- If Sentry is configured, you also have to set `EN_SENTRY_ENVIRONMENT` variable to configure the environment in events
  reported to Sentry.
