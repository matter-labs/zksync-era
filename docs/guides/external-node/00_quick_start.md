# Quick Start

## Prerequisites

Install `docker compose` and `Docker`

## Running ZkSync external node locally

To start a mainnet instance, run:

```sh
cd docker-compose-examples
docker compose --file mainnet-external-node-docker-compose.yml up
```

To reset its state, run:

```sh
cd docker-compose-examples
docker compose --file mainnet-external-node-docker-compose.yml down --volumes
```

To start a testnet instance, run:

```sh
cd docker-compose-examples
docker compose --file testnet-external-node-docker-compose.yml up
```

To reset its state, run:

```sh
cd docker-compose-examples
docker compose --file testnet-external-node-docker-compose.yml down --volumes
```

You can see the status of the node (after recovery) in
[local grafana dashboard](http://localhost:3000/d/0/external-node).

Those commands start external node locally inside docker.

The HTTP JSON-RPC API can be accessed on port `3060` and WebSocket API can be accessed on port `3061`.

> [!NOTE]
>
> The node will recover from a snapshot on it's first run, this may take up to 10h. Before the recovery is finished, the
> API server won't serve any requests.

> [!NOTE]
>
> If you need access to historical transaction data, please use recovery from DB dumps (see Advanced setup section)

### System Requirements

- 32 GB of RAM and a relatively modern CPU
- 50GB of storage for testnet instance
- 700GB of storage for mainnet instance

## Advanced setup

If you need monitoring, backups, to recover from DB dump or a more customized postgres settings, etc, please see:
[ansible-en-role repo](https://github.com/matter-labs/ansible-en-role)
