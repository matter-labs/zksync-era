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

## Building & pushing

Use the `External Node - Build & push docker image` GitHub action. By default, it'll publish the image with `latest2.0`
tag.
