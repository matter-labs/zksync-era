# zkSync Cross External Nodes Consistency Checker

This tool is used to check the consistency of external node instances against the main node. The tool has two main
checkers:

1. RPC Checker, which checks the consistency of the RPC API of the external node against the main node.
2. PubSub Checker, which checks the consistency of the PubSub API of the external node against the main node.

Without any arguments, the tool will run both checkers. The RPC Checker will run in Triggered mode, checking all
available blocks, and the PubSub Checker will run for as long as the RPC Checker is working.

Do note that for the PubSub Checker to properly check the consistency between the nodes, enough time needs to pass. That
is because the PubSub clients may start out of sync. Minimal recommended amount of time for the PubSub Checker is 80
seconds, which would guarantee at least 20 miniblocks checked.

## Running locally

Run the server

```
zk init
zk server --components api,tree,eth,data_fetcher,state_keeper
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
zk run cross-en-checker
```
