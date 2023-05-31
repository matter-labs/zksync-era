# Running the External Node

This section assumes that you have prepared a configuration file as described on the
[previous page](./02_configuration.md).

## Preferred hardware configuration

This configuration is approximate, expect updates to these specs.

- 32-core CPU
- 32GB RAM
- 400GB SSD storage (NVMe recommended)
- 100 Mbps network connection.

## Infrastructure

You need to set up a PostgreSQL server capable of holding at least 1TB of data.

You are expected to have a DB dump from a corresponding env. You can restore it using
`pg_restore -O -C <DUMP_PATH> --dbname=<DB_URL>`.

## Running

Assuming you have the EN Docker image and an env file with the prepared configuration, that is all you need.

Sample running command:

```sh
docker run <image> --env-file <path_to_env_file> --mount type=bind,source=<local_rocksdb_path>,target=<configured_db_path>
```

Helm charts and other infrastructure configuration options, if required, would be available later.

## First start

When you start the node for the first time, the state in PostgreSQL corresponds to the dump you have used, but the state
in RocksDB (mainly the Merkle tree) is absent. Before the node can make any progress, it has to rebuild the state in
RocksDB and verify consistency. The exact time required for that depends on the hardware configuration, but it is
reasonable to expect the state rebuild on the mainnet to take more than 8 hours.

Monitoring the node behavior and analyzing the state it's in is covered in the
[observability section](./04_observability.md).
