# External Node Configuration

This document outlines various configuration options for the EN. Currently, the EN requires the definition of numerous
environment variables. To streamline this process, we provide prepared configs for the zkSync Era - for both
[mainnet](prepared_configs/mainnet-config.env) and [testnet](prepared_configs/testnet-config.env). You can use these
files as a starting point and modify only the necessary sections.

## Database

The EN uses two databases: PostgreSQL and RocksDB.

PostgreSQL serves as the main source of truth in the EN, so all the API requests fetch the state from there. PostgreSQL
connection is configured by the `DATABASE_URL`. Additionally, the `DATABASE_POOL_SIZE` variable defines the size of the
connection pool.

RocksDB is used in components where IO is a bottleneck, such as the State Keeper and the Merkle tree. If possible, it is
recommended to use an NVME SSD for RocksDB. RocksDB requires two variables to be set: `DATABASE_STATE_KEEPER_DB_PATH`
and `DATABASE_NEW_MERKLE_TREE_SSD_PATH`, which must point to different directories.

## L1 Web3 client

EN requires a connection to an Ethereum node. The corresponding env variable is `ETH_CLIENT_WEB3_URL`. Make sure to set
the URL corresponding to the correct L1 network (L1 mainnet for L2 mainnet and L1 goerli for L2 testnet).

Note: Currently, the EN makes 2 requests to the L1 per L1 batch, so the Web3 client usage for a synced node should not
be high. However, during the synchronization phase the new batches would be persisted on the EN quickly, so make sure
that the L1 client won't exceed any limits (e.g. in case you use Infura).

## Exposed ports

The dockerized version of the server exposes the following ports:

- HTTP JSON-RPC: 3060
- WebSocket JSON-RPC: 3061
- Prometheus listener: 3322
- Healtcheck server: 3081

While the configuration variables for them exist, you are not expected to change them unless you want to use the EN
outside of provided docker environment (not supported at the time of writing).

## API limits

A set of variables prefixed with `API_WEB3_JSON_RPC_` allows you to fine-tune the limits of the RPC servers, such as
limits on the number of returned entries or the limit for the accepted transaction size. Provided files contain sane
defaults that are recommended for use, but these can be edited, e.g. to make the EN more/less restrictive.

## Logging and observability

`MISC_LOG_FORMAT` defines the format in which logs are shown: `plain` corresponds to the human-readable format, while
the other option is `json` (recommended for deployments).

`RUST_LOG` variable allows you to set up the logs granularity (e.g. make the EN emit fewer logs). You can read about the
format [here](https://docs.rs/env_logger/0.10.0/env_logger/#enabling-logging).

`MISC_SENTRY_URL` and `MISC_OTLP_URL` variables can be configured to set up Sentry and OpenTelementry exporters.
