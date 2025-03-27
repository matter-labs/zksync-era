# Quick Start

## Prerequisites

Install `docker compose` and `Docker`

## Running ZKsync node locally

These commands start ZKsync Node locally inside docker.

For adjusting the Dockerfiles to use them with other chains setup using ZK Stack, see
[setup_for_other_chains](11_setup_for_other_chains.md)

```admonish note
If you want to run Node for a chain different than ZKsync ERA, you can ask the company hosting the chains for the
ready docker-compose files.
```

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

### Observability

You can see the status of the node (after recovery) in [local grafana dashboard](http://localhost:3000/dashboards). You
can also access a debug page with more information about the node [here](http://localhost:5000).

The HTTP JSON-RPC API can be accessed on port `3060` and WebSocket API can be accessed on port `3061`.

```admonish note
The node will recover from a snapshot on it's first run, this may take up to 10h. Before the recovery is finished, the
API server won't serve any requests.

If you need access to historical transaction data, please use recovery from DB dumps (see Advanced setup section)
```

### System Requirements

```admonish note
Those are requirements for nodes that use snapshots recovery and history pruning (the default for docker-compose
setup).

For requirements for nodes running from DB dump see the [running](03_running.md) section. DB dumps are a way to start
Node with full historical transactions history.

For nodes with pruning disabled, expect the storage requirements on mainnet to grow at 1TB per month. If you want to
stop historical DB pruning you can read more about this in the [pruning](08_pruning.md) section.
```

```admonish note
For chains other than ZKSync Era, the system requirements can be slightly lower (CPU and RAM) or even much lower
(storage), depending on the chain.
```

- 32 GB of RAM and a relatively modern CPU
- 50 GB of storage for testnet nodes
- 500 GB of storage for mainnet nodes
- 100 Mbps connection (1 Gbps+ recommended)

## Advanced setup

If you need monitoring, backups, to recover from DB dump or a more customized PostgreSQL settings, etc, please see:
[ansible-en-role repo](https://github.com/matter-labs/ansible-en-role)
