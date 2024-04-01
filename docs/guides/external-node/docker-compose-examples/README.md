# Running External node locally

This directory contains two docker compose files for running external nodes, for sepolia testnet and mainnet.

They are prepared to work out-of-the-box, no config changes are needed

By default, the HTTP JSON-RPC API will run on port `3050`, while WS API will run on port `3051`.

**Note: The node will recover from a snapshot on it's first run, this may take up to 10h.**

**It needs to finish recovery, before the API server can serve any requests.**

## Dependencies

To run external node locally, you must have `docker compose` and `Docker` installed on your machine.

## Usage

To start testnet mainnet external node, run:

```
> docker compose --file mainnet-docker-external-node-compose.yml up
```

To start testnet sepolia external node, run:

```
> docker compose --file testnet-sepolia-external-node-docker-compose.yml up
```

### More complex setup

This docker compose setup is very simple, without monitoring, backups, with default postgres settings, etc.

**If you need something more advanced, please see:** [ansible-en-role repo](https://github.com/matter-labs/ansible-en-role)
