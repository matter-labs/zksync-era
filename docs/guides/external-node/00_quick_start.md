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

This configuration is approximate and should be considered as *minimal* requirements.

- 32-core CPU
- 64GB RAM
- SSD storage (NVME recommended):
  - Sepolia Testnet - 10GB EN + 50GB PostgreSQL (at the time of writing, will grow over time, so should be constantly monitored)
  - Mainnet - 3TB EN + 7TB PostgreSQL (at the time of writing, will grow over time, so should be constantly monitored)
- 100 Mbps connection (1 Gbps+ recommended)

## Advanced setup

If you need monitoring, backups, to recover from DB dump/snapshot or a more customized PostgreSQL settings, etc, please see:
[ansible-en-role repo](https://github.com/matter-labs/ansible-en-role)
