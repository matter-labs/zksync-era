# zkSync Cross External Nodes Consistency Checker

This tool is used to check the consistency of external node instances against the main node.

## Running locally

Currently, the URLs to the nodes are set in the main file, so ensure that you have the following consts set to the nodes
you want to check:

```bash
EN_INSTANCES_URLS="http://127.0.0.1:3060"
MAIN_NODE_URL ="http://127.0.0.1:3050"
```

Run the server

```
zk init
zk server --components api,tree_lightweight,eth,data_fetcher,state_keeper
```

Run the EN

```
zk env ext-node
zk clean --database
zk db setup
zk external-node
```

Run integration tests to populate the main node with data.

```
zk test i server
```

Run the checker

```
cd core/tests/cross_external_nodes_checker
CHECKER_MODE={Continuous/Triggered} CHECKER_MAIN_NODE_URL={MAIN_NODE_URL}
CHECKER_INSTANCES_URLS={EN_INSTANCES_URLS} CHECKER_INSTANCE_POLL_PERIOD={POLL_PERIOD}
cargo run
```

Examples:

```
# Continuous Mode connecting to local main node, local EN, and stage EN.
    CHECKER_MODE=Continuous CHECKER_MAIN_NODE_URL="http://127.0.0.1:3050"
    CHECKER_INSTANCES_URLS="http://127.0.0.1:3060","https://external-node-dev.zksync.dev:443"
    CHECKER_INSTANCE_POLL_PERIOD=10 RUST_LOG=cross_external_nodes_checker::checker=debug cargo run

# Triggered Mode with start and finish miniblocks to check.
    CHECKER_MODE=Triggered CHECKER_START_MINIBLOCK=0 CHECKER_FINISH_MINIBLOCK=10
    CHECKER_MAIN_NODE_URL="http://127.0.0.1:3050" CHECKER_INSTANCES_URLS="http://127.0.0.1:3060"
    CHECKER_INSTANCE_POLL_PERIOD=10 RUST_LOG=info cargo run
```
