# Decentralization

On the gossipnet, the data integrity will be protected by the BFT (byzantine fault-tolerant) consensus algorithm
(currently data is signed just by the main node though).

## Enabling gossipnet on your node

```admonish note
The minimal supported server version for this is
[24.11.0](https://github.com/matter-labs/zksync-era/releases/tag/core-v24.11.0)
```

### Generating secrets

Each participant node of the gossipnet has to have an identity (a public/secret key pair). When running your node for
the first time, generate the secrets by running:

```
docker run --entrypoint /usr/bin/zksync_external_node "matterlabs/external-node:2.0-v28.2.1" generate-secrets > consensus_secrets.yaml
chmod 600 consensus_secrets.yaml
```

```admonish danger
NEVER reveal the secret keys used by your node. Otherwise, someone can impersonate your node on the gossipnet. If you
suspect that your secret key has been leaked, you can generate fresh keys using the same tool.

If you want someone else to connect to your node, give them your PUBLIC key instead. Both public and secret keys are
present in the `consensus_secrets.yaml` (public keys are in comments).
```

### Preparing configuration file

Copy the template of the consensus configuration file (for
[mainnet](https://github.com/matter-labs/zksync-era/blob/main/docs/src/guides/external-node/prepared_configs/mainnet_consensus_config.yaml)
or
[testnet](https://github.com/matter-labs/zksync-era/blob/main/docs/src/guides/external-node/prepared_configs/testnet_consensus_config.yaml)
).

```admonish note
You need to fill in the `public_addr` field. This is the address that will (not implemented yet) be advertised over
gossipnet to other nodes, so that they can establish connections to your node. If you don't want to expose your node
to the public internet, you can use IP in your local network.
```

Currently, the config contains the following fields:

- `server_addr` - local TCP socket address that the node should listen on for incoming connections. Note that this is an
  additional TCP port that will be opened by the node.
- `public_addr` - the public address of your node that will be advertised over the gossipnet.
- `max_payload_size` - limit (in bytes) on the sized of the ZKsync ERA block received from the gossipnet. This protects
  your node from getting DoS`ed by too large network messages. Use the value from the template.
- `gossip_dynamic_inbound_limit` - maximal number of unauthenticated concurrent inbound connections that can be
  established to your node. This is a DDoS protection measure.

### Setting environment variables

Uncomment or add the following lines in your `.env` config:

```
EN_CONSENSUS_CONFIG_PATH=...
EN_CONSENSUS_SECRETS_PATH=...
```

These variables should point to your consensus config and secrets files that we have just created. Tweak the paths to
the files if you have placed them differently.
