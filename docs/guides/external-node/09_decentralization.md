# Decentralization

In the default setup the ZKsync node will fetch data from the ZKsync API endpoint maintained by Matter Labs. To reduce
the reliance on this centralized endpoint we have developed a decentralized p2p networking stack (aka gossipnet) which
will eventually be used instead of ZKsync API for synchronizing data.

On the gossipnet, the data integrity will be protected by the BFT (byzantine fault tolerant) consensus algorithm
(currently data is signed just by the main node though).

## Enabling gossipnet on your node

> [!NOTE]
>
> Because the data transmitted over the gossipnet is signed by the main node (and eventually by the consensus quorum),
> the signatures need to be backfilled to the node's local storage the first time you switch from centralized (ZKsync
> API based) synchronization to the decentralized (gossipnet based) synchronization (this is a one-time thing). With the
> current implementation it may take a couple of hours and gets faster the more nodes you add to the
> `gossip_static_outbound` list (see below). We are working to remove this inconvenience.

### Generating secrets

Each participant node of the gossipnet has to have an identity (a public/secret key pair). When running your node for the
first time, generate the secrets by running:

```
cargo run -p zksync_external_node -- generate-secrets > consensus_secrets.yaml
```

> [!NOTE]
>
> NEVER reveal the secret keys used by your node. Otherwise someone can impersonate your node on the gossipnet. If you
> suspect that your secret key has been leaked, you can generate fresh keys using the same tool.
>
> If you want someone else to connect to your node, give them your PUBLIC key instead. Both public and secret keys are
> present in the `consensus_secrets.yaml` (public keys are in comments).

### Preparing configuration file

Copy the template of the consensus configuration file (for [mainnet](perpared_configs/mainnet_consensus_config.yaml) or
[testnet](perpared_configs/testnet_consensus_config.yaml) ).

> [!NOTE]
>
> You need to fill in the `public_addr` field. This is the address that will (not implemented yet) be advertised over
> gossipnet to other nodes, so that they can establish connections to your node. If you don't want to expose your node
> to the public internet, you can use IP in your local network.

Currently the config contains the following fields (refer to config
[schema](https://github.com/matter-labs/zksync-era/blob/990676c5f84afd2ff8cd337f495c82e8d1f305a4/core/lib/protobuf_config/src/proto/core/consensus.proto#L66)
for more details):

- `server_addr` - local TCP socket address that the node should listen on for incoming connections. Note that this is an
  additional TCP port that will be opened by the node.
- `public_addr` - the public address of your node that will be advertised over the gossipnet.
- `max_payload_size` - limit (in bytes) on the sized of the ZKsync ERA block received from the gossipnet. This protects
  your node from getting DoS`ed by too large network messages. Use the value from the template.
- `gossip_dynamic_inbound_limit` - maximal number of unauthenticated concurrent inbound connections that can be
  established to your node. This is a DDoS protection measure.
- `gossip_static_outbound` - list of trusted peers that your node should always try to connect to. The template contains
  the nodes maintained by Matterlabs, but you can add more if you know any. Note that the list contains both the network
  address AND the public key of the node - this prevents spoofing attacks.

### Setting environment variables

Uncomment (or add) the following lines in your `.env` config:

```
EN_CONSENSUS_CONFIG_PATH=...
EN_CONSENSUS_SECRETS_PATH=...
```

These variables should point to your consensus config and secrets files that we have just created. Tweak the paths to
the files if you have placed them differently.

### Add `--enable-consensus` flag to your entry point

For the consensus configuration to take effect you have to add `--enable-consensus` flag to the command line when
running the node:

```
cargo run -p zksync_external_node -- <all the other flags> --enable-consensus
```
