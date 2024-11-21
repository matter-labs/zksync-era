# Decentralization

To enable support for synchronization over p2p network, the main node needs to have the "consensus" component configured
and enabled as follows:

## Generating the consensus secrets

Run the following to generate consensus secrets:

```
docker run --entrypoint /usr/bin/zksync_external_node "matterlabs/external-node:2.0-v25.0.0" generate-secrets > consensus_secrets.yaml
chmod 600 consensus_secrets.yaml
```

## Preparing the consensus config

Create `consensus_config.yaml` file with the following content (remember to replace the placeholders):

```yaml
server_addr: '0.0.0.0:3054'
public_addr:
  # Address under which the node is accessible to the other nodes.
  # It can be a public domain, like `example.com:3054`, in case the main node is accessible from the internet,
  # or it can be a kubernetes cluster domain, like `server-v2-core.<cluster name>.svc.cluster.local:3054` in
  # case the main node should be only accessible within the cluster.
debug_page_addr: '0.0.0.0:5000'
max_payload_size: 3200000
gossip_dynamic_inbound_limit: 10
genesis_spec:
  chain_id: # chain id
  protocol_version: 1 # consensus protocol version
  validators:
    - key: validator:public:??? # public key of the main node (copy this PUBLIC key from consensus_secrets.yaml)
      weight: 1
  leader: validator:public:??? # same as above - main node will be the only validator and the only leader.
```

## Providing the configuration to the `zksync_server`

To enable consensus component for the main node you need to append
`--components=<whatever components you were running until now>,consensus` to the `zksync_server` command line arguments.
In addition to that, you need to provide the configuration (from the files `consensus_config.yaml` and
`consensus_secrets.yaml` that we have just prepared) to the `zksync_server` binary. There are 2 ways (hopefully not for
long) to achieve that:

- In file-based configuration system, the consensus config is embedded in the
  [general config](https://github.com/matter-labs/zksync-era/blob/1edcabe0c6a02d5b6700c29c0d9f6220ec6fb03c/core/lib/config/src/configs/general.rs#L58),
  and the consensus secrets are embedded in the
  [secrets config](https://github.com/matter-labs/zksync-era/blob/main/core/bin/zksync_server/src/main.rs). Paste the
  content of the generated `consensus_secrets.yaml` file to the `secrets` config, and prepared config to the `general`
  config.

- In env-var-based configuration system, the consensus config and consensus secrets files are passed as standalone
  files. The paths to these files need to be passed as env vars `CONSENSUS_CONFIG_PATH` and `CONSENSUS_SECRETS_PATH`.

## Gitops repo config

If you are using the matterlabs gitops repo to configure the main node, it is even more complicated because the
`consensus_config.yaml` file is rendered from a helm chart. See the
[example](https://github.com/matter-labs/gitops-kubernetes/blob/main/apps/environments/mainnet2/server-v2/server-v2-core.yaml),
to see where you have to paste the content of the `consensus_config.yaml` file.

You need to embed the `consensus_secrets.yaml` file into a kubernetes config:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: consensus-secrets
type: Opaque
stringData:
  .consensus_secrets.yaml: <here goes the content of the consensus_secrets.yaml file>
```

You need to add the following sections to your kubernetes config for the core server:

```yaml
spec:
  values:
    persistence:
      consensus-secrets-volume:
        name: consensus-secrets # this is the name of the secret kubernetes object we defined above
        enabled: true
        type: secret
        mountPath: '/etc/consensus_secrets/'
    args:
      - --components=state_keeper,consensus
    service:
      main:
        ports:
          consensus:
            enabled: true
            port: 3054
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
