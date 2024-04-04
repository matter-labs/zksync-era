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

**Note: The node will first recover from a snapshot on it's first run, this may take up to 10h.**

**Note: If you need access to historical transaction data, please use recovery from DB dumps (see Advanced setup
section)**

### System Requirements

- 32 GB of RAM and a relatively modern CPU
- 50GB of storage for testnet instance
- 700GB of storage for mainnet instance

## Advanced setup

If you need monitoring, backups, to recover from DB dump or a more customised postgres settings, etc, please see:
[ansible-en-role repo](https://github.com/matter-labs/ansible-en-role)
