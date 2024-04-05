# Quick Start

## Prerequisites

Install `docker compose` and `Docker`

## Running ZkSync external node locally

To start a mainnet instance, run:

```sh
cd docker-compose-examples
docker compose --file mainnet-docker-external-node-compose.yml up
```

To start testnet instance, run:

```sh
cd docker-compose-examples
docker compose --file testnet-external-node-docker-compose.yml up
```

Those commands start external node locally inside docker.

The HTTP JSON-RPC API can be accessed on port `3060` and WebSocket API can be accessed on port `3061`.

<!-- prettier-ignore-start -->
> [!NOTE]
> The node will recover from a snapshot on it's first run, this may take up to 10h. Before the recovery is
> finished, the API server won't serve any requests.
<!-- prettier-ignore-end --> 
<!-- prettier-ignore-start -->
> [!NOTE]
> If you need access to historical transaction data, please use recovery from DB dumps (see Advanced setup
> section)
<!-- prettier-ignore-end -->

### System Requirements

- 32 GB of RAM and a relatively modern CPU
- 50GB of storage for testnet instance
- 700GB of storage for mainnet instance

## Advanced setup

If you need monitoring, backups, to recover from DB dump or a more customized postgres settings, etc, please see:
[ansible-en-role repo](https://github.com/matter-labs/ansible-en-role)
