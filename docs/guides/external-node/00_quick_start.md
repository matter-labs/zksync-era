# Quick Start

## Prerequisites

Install `docker compose` and `Docker`

## Running ZKsync node locally

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

Those commands start ZKsync node locally inside docker.

The HTTP JSON-RPC API can be accessed on port `3060` and WebSocket API can be accessed on port `3061`.

> [!NOTE]
>
> The node will recover from a snapshot on it's first run, this may take up to 10h. Before the recovery is finished, the
> API server won't serve any requests.

> [!NOTE]
>
> If you need access to historical transaction data, please use recovery from DB dumps (see Advanced setup section)

### System Requirements

> [!NOTE]
>
> This configuration is only for nodes that use snapshots recovery (the default for docker-compose setup), for
> requirements for nodes running from DB dump see
> [03_running.md](https://github.com/matter-labs/zksync-era/blob/main/docs/guides/external-node/03_running.md). DB dumps
> are a way to start ZKsync node with full historical transactions history

> [!NOTE]
>
> Those are requirements for a freshly started node and the the state grows about 1TB per month for mainnet

> [!NOTE]
>
> To stop state growth, you can enable state pruning by uncommenting `EN_PRUNING_ENABLED: true` in docker compose file,
> you can read more about pruning in
> [08_pruning.md](https://github.com/matter-labs/zksync-era/blob/main/docs/guides/external-node/08_pruning.md)

- 32 GB of RAM and a relatively modern CPU
- 30 GB of storage for testnet nodes
- 300 GB of storage for mainnet nodes
- 100 Mbps connection (1 Gbps+ recommended)

## Advanced setup

If you need monitoring, backups, to recover from DB dump or a more customized PostgreSQL settings, etc, please see:
[ansible-en-role repo](https://github.com/matter-labs/ansible-en-role)
