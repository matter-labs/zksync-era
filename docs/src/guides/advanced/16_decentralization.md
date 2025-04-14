# Decentralization

To enable support for synchronization over p2p network, the main node needs to have the "consensus" component configured
and enabled as follows:

## Generating the consensus secrets

Run the following to generate consensus secrets:

```
docker run --entrypoint /usr/bin/zksync_external_node "matterlabs/external-node:2.0-v25.0.0" generate-secrets
```

That will output something like this (but with different keys obviously):

```
#validator:public:bls12_381:84fe19a96b6443ca7ce...98dec0870f6d8aa95c8164102f0d62e4c47e3566c4e5c32354d
validator_key: validator:secret:bls12_381:1de85683e6decbfcf6c12aa42a5c8bfa98d7ae796dee068ae73dc784a58f5213
# node:public:ed25519:acb7e350cf53e3b4c2042e2c8044734384cee51f58a0fa052fd7e0c9c3f4b20d
node_key: node:secret:ed25519:0effb1d7c335d23606f656ca1ba87566144d5af2984bd7486379d4f83a204ba2
```

You then have two different paths depending if your main node is using file-based or env-based configuration.

## Configuring consensus

### File-based configuration

If you are using the recommended file-based configuration then you'll need to add the following information to your
`general.yaml` config file (see [Ecosystem Configuration](../launch.md#ecosystem-configuration)):

```yaml
consensus:
  server_addr: '0.0.0.0:3054'
  public_addr:
    '???'
    # Address under which the node is accessible to the other nodes.
    # It can be a public domain, like `example.com:3054`, in case the main node is accessible from the internet,
    # or it can be a kubernetes cluster domain, like `server-v2-core.<cluster name>.svc.cluster.local:3054` in
    # case the main node should be only accessible within the cluster.
  debug_page_addr: '0.0.0.0:5000'
  max_payload_size: 3200000
  gossip_dynamic_inbound_limit: 10
  genesis_spec:
    chain_id: ??? # chain id
    protocol_version: 1 # consensus protocol version
    validators:
      - key: validator:public:??? # validator public key of the main node (copy this PUBLIC key from the secrets you generated)
        weight: 1
    leader: validator:public:??? # same as above - main node will be the only validator and the only leader.
    seed_peers:
      - key: 'node:public:ed25519:...' # node public key of the main node (copy this PUBLIC key from the secrets you generated)
        addr: '???' # same as public_addr above
```

And the secrets you generated to your `secrets.yaml` config file:

```yaml
consensus:
  validator_key: validator:secret:???
  node_key: node:secret:???
```

### Env-based configuration

If you are using the env-based configuration you'll need to create a `consensus_config.yaml` file with the following
content:

```yaml
server_addr: '0.0.0.0:3054'
public_addr:
  '???'
  # Address under which the node is accessible to the other nodes.
  # It can be a public domain, like `example.com:3054`, in case the main node is accessible from the internet,
  # or it can be a kubernetes cluster domain, like `server-v2-core.<cluster name>.svc.cluster.local:3054` in
  # case the main node should be only accessible within the cluster.
debug_page_addr: '0.0.0.0:5000'
max_payload_size: 3200000
gossip_dynamic_inbound_limit: 10
genesis_spec:
  chain_id: ??? # chain id
  protocol_version: 1 # consensus protocol version
  validators:
    - key: validator:public:??? # validator public key of the main node (copy this PUBLIC key from the secrets you generated)
      weight: 1
  leader: validator:public:??? # same as above - main node will be the only validator and the only leader.
  seed_peers:
    - key: 'node:public:ed25519:...' # node public key of the main node (copy this PUBLIC key from the secrets you generated)
      addr: '???' # same as public_addr above
```

And a `consensus_secrets.yaml` file with the with the secrets you generated previously:

```yaml
validator_key: validator:secret:???
node_key: node:secret:???
```

Don't forget to set secure permissions to it:

```
chmod 600 consensus_secrets.yaml
```

Then you'll need to pass the paths to these files as env vars `CONSENSUS_CONFIG_PATH` and `CONSENSUS_SECRETS_PATH`.

## Running the `zksync_server`

Finally, to enable the consensus component for the main node you just need to append
`--components=<whatever components you were running until now>,consensus` to the `zksync_server` command line arguments.

## Gitops repo config

If you are using the matterlabs gitops repo to configure the main node, you'll need to add this information to your
kubernetes config for the core server, `server-v2-core.yaml` file (see
[example](https://github.com/matter-labs/gitops-kubernetes/blob/177dcd575c6ab446e70b9a9ced8024766095b516/apps/environments/era-stage-proofs/server-v2/server-v2-core.yaml#L23-L35)):

```yaml
spec:
  values:
    args:
      - --components=state_keeper,consensus
    service:
      main:
        ports:
          consensus:
            enabled: true
            port: 3054
```

Then again you have two paths depending if the deployment is using file-based or env-based configuration. Although by
default you should be using file-based configuration.

### File-based configuration

Just like before you'll add the consensus config information to the `general.yaml` config file (see
[example](https://github.com/matter-labs/gitops-kubernetes/blob/177dcd575c6ab446e70b9a9ced8024766095b516/apps/environments/era-stage-proofs/server-v2-config/general.yaml#L353-L368)).

And the secrets you generated to your whatever secrets managing system you are using (see an example
[here](https://github.com/matter-labs/gitops-kubernetes/blob/177dcd575c6ab446e70b9a9ced8024766095b516/apps/clusters/era-stage-proofs/stage2/secrets/server-v2-secrets.yaml)
using SOPS).

```yaml
consensus:
  validator_key: validator:secret:???
  node_key: node:secret:???
```

### Env-based configuration

It is even more complicated because the `consensus_config.yaml` file is rendered from a helm chart. See the
[example](https://github.com/matter-labs/gitops-kubernetes/blob/177dcd575c6ab446e70b9a9ced8024766095b516/apps/environments/mainnet2/server-v2/server-v2-core.yaml#L37-L92),
to see where you have to paste the content of the `consensus_config.yaml` file.

You also need to add the following sections to your `server-v2-core.yaml` file:

```yaml
spec:
  values:
    persistence:
      consensus-secrets-volume:
        name: consensus-secrets # this is the name of the secret kubernetes object we defined above
        enabled: true
        type: secret
        mountPath: '/etc/consensus_secrets/'
    configMap:
      consensus:
        enabled: true
        data:
          consensus_config.yaml: <here goes the content of the consensus_config.yaml file>
    env:
      - name: CONSENSUS_CONFIG_PATH
        value: /etc/consensus_config.yaml # this is the location rendered by the helm chart, you can't change it
      - name: CONSENSUS_SECRETS_PATH
        value: /etc/consensus_secrets/.consensus_secrets.yaml
```

You need to embed the `consensus_secrets.yaml` file into a kubernetes config (see how to do it
[here](https://github.com/matter-labs/gitops-kubernetes/blob/177dcd575c6ab446e70b9a9ced8024766095b516/apps/environments/mainnet2/zksync-v2-secret/kustomization.yaml#L3-L4)
and
[here](https://github.com/matter-labs/gitops-kubernetes/blob/177dcd575c6ab446e70b9a9ced8024766095b516/apps/environments/mainnet2/zksync-v2-secret/consensus_secrets.yaml)):

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: consensus-secrets
type: Opaque
stringData:
  .consensus_secrets.yaml: <here goes the content of the consensus_secrets.yaml file>
```
