# Quick Start

## Prerequisites

Install `docker compose` and `Docker`

## Running ZKsync node locally

To start a mainnet instance, run:

```sh
cd docker-compose-examples
sudo docker compose --file mainnet-external-node-docker-compose.yml up
```

To reset its state, run:

```sh
cd docker-compose-examples
sudo docker compose --file mainnet-external-node-docker-compose.yml down --volumes
```

To start a testnet instance, run:

```sh
cd docker-compose-examples
sudo docker compose --file testnet-external-node-docker-compose.yml up
```

To reset its state, run:

```sh
cd docker-compose-examples
sudo docker compose --file testnet-external-node-docker-compose.yml down --volumes
```

You can see the status of the node (after recovery) in [local grafana dashboard](http://localhost:3000/dashboards).

Those commands start ZKsync node locally inside docker.

The HTTP JSON-RPC API can be accessed on port `3060` and WebSocket API can be accessed on port `3061`.

> [!NOTE]
>
> The node will recover from a snapshot on it's first run, this may take up to 10h. Before the recovery is finished, the
> API server won't serve any requests.
>
> If you need access to historical transaction data, please use recovery from DB dumps (see Advanced setup section)

### System Requirements

> [!NOTE]
>
> Those are requirements for nodes that use snapshots recovery and history pruning (the default for docker-compose
> setup).
>
> For requirements for nodes running from DB dump see the [running](03_running.md) section. DB dumps are a way to start
> ZKsync node with full historical transactions history.
>
> For nodes with pruning disabled, expect the storage requirements on mainnet to grow at 1TB per month. If you want to
> stop historical DB pruning you can read more about this in the [pruning](08_pruning.md) section.

- 32 GB of RAM and a relatively modern CPU
- 50 GB of storage for testnet nodes
- 500 GB of storage for mainnet nodes
- 100 Mbps connection (1 Gbps+ recommended)

## Advanced setup

If you need monitoring, backups, to recover from DB dump or a more customized PostgreSQL settings, etc, please see:
[ansible-en-role repo](https://github.com/matter-labs/ansible-en-role)
