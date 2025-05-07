# ZKsync External Node

This application is a read replica that can sync from the main node and serve the state locally.

Note: this README is under construction.

## Local development

This section describes how to run the external node locally.

### Configuration

Right now, external node requires all the configuration parameters that are required for the main node. It also has one
unique parameter: `API_WEB3_JSON_RPC_MAIN_NODE_URL` -- the address of the main node to fetch the state from.

The easiest way to see everything that is used is to compile the `ext-node` config and see the contents of the resulting
`.env` file.

Note: not all the config values from the main node are actually used, so this is temporary, and in the future external
node would require a much smaller set of config variables.

To change the configuration, edit the `etc/env/chains/ext-node.toml`, add the overrides from the `base` config if you
need any. Remove `etc/env/chains/ext-node.env`, if it exists. On the next launch of the external node, new config would
be compiled and will be written to the `etc/env/chains/ext-node.env` file.

### Running

To run the binary:

```sh
ZKSYNC_ENV=ext-node zk f cargo run --release --bin zksync_external_node
```

### Clearing the state

This command will reset the Postgres and RocksDB databases used by the external node:

```sh
ZKSYNC_ENV=ext-node zk db reset && rm -r $ZKSYNC_HOME/en_db
```

## Contract verification api

Run components `all` or `contract_verification_api`

```bash
zksync_external_node --components=all
```

Or, if you want to run only the Contract Verification API component:

```bash
zksync_external_node --components=contract_verification_api
```

### Configuration

#### Environment variables

Example env configuration

```
DATABASE_URL: "postgres://postgres:notsecurepassword@postgres:5430/zksync_local_ext_node"
DATABASE_POOL_SIZE: 10

EN_HTTP_PORT: 3060
EN_WS_PORT: 3061
EN_HEALTHCHECK_PORT: 3081
EN_PROMETHEUS_PORT: 3322
EN_ETH_CLIENT_URL: http://localhost:8545
EN_MAIN_NODE_URL: http://localhost:3050
EN_L1_CHAIN_ID: 9
EN_L2_CHAIN_ID: 111
EN_PRUNING_ENABLED: true

EN_STATE_CACHE_PATH: "./db/ext-node/state_keeper"
EN_MERKLE_TREE_PATH: "./db/ext-node/lightweight"
RUST_LOG: "warn,zksync=info,zksync_core::metadata_calculator=debug,zksync_state=debug,zksync_utils=debug,zksync_web3_decl::client=error"

EN_CONSENSUS_CONFIG_PATH: "/configs/testnet_consensus_config.yaml"
EN_CONSENSUS_SECRETS_PATH: "/configs/testnet_consensus_secrets.yaml"

EN_CONTRACT_VERIFIER_COMPILATION_TIMEOUT: 3600
EN_CONTRACT_VERIFIER_PORT: 3070
EN_CONTRACT_VERIFIER_PROMETHEUS_PORT: 8080
```

consensus_config.yaml example

```yaml
server_addr: 'localhost:3054'
public_addr: 'localhost:3054'
debug_page_addr: '0.0.0.0:5000'
main_node_url: 'http://localhost:3050'
l1_chain_id: 9
l2_chain_id: 111
l1_batch_commit_data_generator_mode: 0
max_payload_size: 5000000
gossip_dynamic_inbound_limit: 100
gossip_static_outbound:
  - key: 'node:public:ed25519:3c0820d291d58a642923d033e474bf8dd783afc43'
    addr: 'localhost:3054'
```

consensus_secrets.yaml example

```yaml
# validator:public:bls12_381:a341d802e6f601a617a107ec82d83ef092e037f31980afce0c57346e644bd6b91703a20506b296ec
validator_key: validator:secret:bls12_381:6328a92399714fbfc4e877b7a579105dfbbcd4e
# attester:public:secp256k1:0200bf7f4b74e4e4ba1eb784e513cb8747437054b3903ff984e45e
attester_key: attester:secret:secp256k1:96d853938d5ed7f79a8f8dfaa45b5744d790db8d459
# node:public:ed25519:706a4879f0c7b16afaf0fe627eff77e3f00703b459f703aa28549
node_key: node:secret:ed25519:60b6b8fe337faeee4
```

### Run contract verifier service

This module runs from the zksync_contract_verifier binary, docker image (matterlabs/contract-verifier) or zkstack
contract-verifier module and is required to validate contracts and populate db with supported zksolc/vyper versions.

Previous example yaml files cover the required config data to run the contract verifier, update and replace the
placeholder values.

## Building & pushing

Use the `External Node - Build & push docker image` GitHub action. By default, it'll publish the image with `latest2.0`
tag.
