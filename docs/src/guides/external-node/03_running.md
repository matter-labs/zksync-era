# Running the Node

```admonish note
If you want to just run ZKSync node with recommended default setting, please see the [quick start](00_quick_start.md)
page.
```

This section assumes that you have prepared a configuration file as described on the
[previous page](02_configuration.md).

## System Requirements for nodes started from DB dumps

This configuration is approximate and should be considered as **minimal** requirements.

- 32-core CPU
- 64GB RAM
- SSD storage (NVME recommended):
  - ZKsync Sepolia Testnet - 10GB Node + 50GB PostgreSQL (at the time of writing, will grow over time, so should be
    constantly monitored)
  - ZKsync Mainnet - 3TB Node + 8TB PostgreSQL (at the time of writing, will grow over time, so should be constantly
    monitored)
- 100 Mbps connection (1 Gbps+ recommended)

For smaller chains, less powerful hardware may be sufficient, especially in terms of disk space.

```admonish tip
RAM requirements [can generally be reduced](02_configuration.md#optimizing-ram-consumption) with a moderate performance tradeoff.
They also may be lower for [non-archive nodes](08_pruning.md) or nodes [without a local Merkle tree](09_treeless_mode.md).
```

## A note about PostgreSQL storage

By far, the heaviest table to maintain is the `call_traces` table. This table is only required for the `debug`
namespace. If you want to clear some space and aren't using the `debug` namespace, you can

- clear it with a simple query `DELETE FROM call_traces;`
- leave the `debug` namespace disabled via the `EN_API_NAMESPACES` env var as described in the
  [example config](prepared_configs/mainnet-config.env).

## Infrastructure

You need to set up a PostgreSQL server, however it is out of the scope of these docs, but the popular choice is to run
it in Docker. There are many of guides on that,
[here's one example](https://www.docker.com/blog/how-to-use-the-postgres-docker-official-image/).

Note however that if you run PostgresSQL as a stand-alone Docker image (e.g. not in Docker-compose with a network shared
between Node and Postgres), Node won't be able to access Postgres via `localhost` or `127.0.0.1` URLs. To make it work,
you'll have to either run it with a `--network host` (on Linux) or use `host.docker.internal` instead of `localhost` in
the Node configuration ([official docs][host_docker_internal]).

Besides running Postgres, you are expected to have a DB dump from a corresponding env. You can restore it using
`pg_restore -O -C <DUMP_PATH> --dbname=<DB_URL>`.

You can also refer to
[Node configuration management blueprint](https://github.com/matter-labs/zksync-era/blob/main/docs/src/guides/external-node/00_quick_start.md#advanced-setup)
for advanced DB instance configurations.

## Running

Assuming you have the Node Docker image, an env file with the prepared configuration, and you have restored your DB with
the pg dump, that is all you need.

Sample running command:

```sh
docker run --env-file <path_to_env_file> --mount type=bind,source=<local_rocksdb_data_path>,target=<configured_rocksdb_data_path> <image>
```

Helm charts and other infrastructure configuration options, if required, would be available later.

## First start

When you start the node for the first time, the state in PostgreSQL corresponds to the dump you have used, but the state
in RocksDB (mainly the Merkle tree) is absent. Before the node can make any progress, it has to rebuild the state in
RocksDB and verify consistency. The exact time required for that depends on the hardware configuration, but it is
reasonable to expect the state rebuild on the mainnet to take more than 20 hours.

## Genesis synchronization

While it's technically possible to synchronize an External Node from genesis, this process is not recommended for any
users. The synchronization algorithm used for External Nodes is designed for keeping the live node up-to-date but
doesn't implement optimizations to re-execute large amounts of historical blocks. As a result, genesis synchronization
on ZKsync Era can take multiple months for a single node, and it is not being continuously tested for each release. The
node is guaranteed to never be in an incorrect state, but issues unrelated to correctness may occur (such as
synchronization getting stuck).

The recommended ways to run the External Node are:

- [Snapshot synchronization](./07_snapshots_recovery.md) for full nodes (if you don't need historical data).
- [Restoration from a DB dump](./00_quick_start.md#advanced-setup) for archival nodes.

## Redeploying the Node with a new PG dump

If you've been running the Node for some time and are going to redeploy it using a new PG dump, you should

- Stop the EN
- Remove SK cache (corresponding to `EN_STATE_CACHE_PATH`)
- Remove your current DB
- Restore with the new dump
- Start the EN

Monitoring the node behavior and analyzing the state it's in is covered in the
[observability section](04_observability.md).

[host_docker_internal]:
  https://docs.docker.com/desktop/networking/#i-want-to-connect-from-a-container-to-a-service-on-the-host
