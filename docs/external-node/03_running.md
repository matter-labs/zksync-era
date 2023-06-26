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

Setting up Postgres is out of the scope of these docs, but the popular choice is to run it in Docker. There are many of
guides on that, [here's one example](https://www.docker.com/blog/how-to-use-the-postgres-docker-official-image/).

Note however that if you run Postgres as a stand-alone Docker image (e.g. not in Docker-compose with a network shared
between EN and Postgres), EN won't be able to access Postgres via `localhost` or `127.0.0.1` URLs. To make it work,
you'll have to either run it with a `--network host` (on Linux) or use `host.docker.internal` instead of `localhost` in
the EN configuration ([official docs][host_docker_internal]).

Besides running Postgres, you are expected to have a DB dump from a corresponding env. You can restore it using
`pg_restore -O -C <DUMP_PATH> --dbname=<DB_URL>`.

[host_docker_internal](https://docs.docker.com/desktop/networking/#i-want-to-connect-from-a-container-to-a-service-on-the-host)

## Running

Assuming you have the EN Docker image and an env file with the prepared configuration, that is all you need.

Sample running command:

```sh
docker run --env-file <path_to_env_file> --mount type=bind,source=<local_rocksdb_data_path>,target=<configured_rocksdb_data_path> <image>
```

Helm charts and other infrastructure configuration options, if required, would be available later.

## First start

When you start the node for the first time, the state in PostgreSQL corresponds to the dump you have used, but the state
in RocksDB (mainly the Merkle tree) is absent. Before the node can make any progress, it has to rebuild the state in
RocksDB and verify consistency. The exact time required for that depends on the hardware configuration, but it is
reasonable to expect the state rebuild on the mainnet to take more than 8 hours.

Monitoring the node behavior and analyzing the state it's in is covered in the
[observability section](./04_observability.md).
