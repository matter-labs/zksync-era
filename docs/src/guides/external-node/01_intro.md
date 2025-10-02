# Node Documentation

The Node (sometimes referred to as External Node or EN) is a read-only replica of the main node.

## What is the Node

The Node is a read-replica of the main (centralized) node that can be run by external parties. It functions by receiving
blocks from the given ZK Stack chain and re-applying transactions locally, starting from the genesis block. The Node
shares most of its codebase with the main node. Consequently, when it re-applies transactions, it does so exactly as the
main node did in the past.

**It has three modes of initialization:**

- recovery from genesis (Not supported on ZKsync Era), in Ethereum terms this corresponds to archival node, this option
  is slower than recovery from DB dump, but is the easiest way to spin up new Node.
- recovery from a DB dump, in Ethereum terms this corresponds to archival node.
- recovery from a snapshot, in Ethereum terms this corresponds to light node, such nodes will only have access to
  transactions data from after the node was initialized. The database can be pruned on such nodes.

## High-level overview

At a high level, the Node can be seen as an application that has the following modules:

- API server that provides the publicly available Web3 interface.
- Consensus layer (ZKsync Era only for now) that interacts with the peer network and retrieves transactions and blocks
  to re-execute.
- Sequencer component that actually executes and persists transactions received from the synchronization layer.
- Several checker modules that ensure the consistency of the Node state.

With the EN, you are able to:

- Locally recreate and verify a ZK Stack chain's, for example ZKsync Era's mainnet/testnet state.
- Interact with the recreated state in a trustless way. The validity is locally verified.
- Use the Web3 API without having to query the main node.
- Send L2 transactions (that will be proxied to the main node).

With the EN, you _can not_:

- Create L2 blocks or L1 batches on your own.
- Generate proofs.
- Submit data to L1.

A more detailed overview of the Node's components is provided in the [components](06_components.md) section.

## API overview

API exposed by the Node strives to be Web3-compliant. If some method is exposed but behaves differently compared to
Ethereum, it should be considered a bug. Please [report][contact_us] such cases.

[contact_us]: https://zksync.io/contact

### `eth` namespace

Data getters in this namespace operate in the L2 space: require/return L2 block numbers, check balances in L2, etc.

Available methods:

| Method                                    | Notes                                                                       |
| ----------------------------------------- | --------------------------------------------------------------------------- |
| `eth_blockNumber`                         |                                                                             |
| `eth_chainId`                             |                                                                             |
| `eth_call`                                |                                                                             |
| `eth_estimateGas`                         |                                                                             |
| `eth_gasPrice`                            |                                                                             |
| `eth_newFilter`                           | Maximum amount of installed filters is configurable                         |
| `eth_newBlockFilter`                      | Same as above                                                               |
| `eth_newPendingTransactionsFilter`        | Same as above                                                               |
| `eth_uninstallFilter`                     |                                                                             |
| `eth_getLogs`                             | Maximum amount of returned entities can be configured                       |
| `eth_getFilterLogs`                       | Same as above                                                               |
| `eth_getFilterChanges`                    | Same as above                                                               |
| `eth_getBalance`                          |                                                                             |
| `eth_getBlockByNumber`                    |                                                                             |
| `eth_getBlockByHash`                      |                                                                             |
| `eth_getBlockTransactionCountByNumber`    |                                                                             |
| `eth_getBlockTransactionCountByHash`      |                                                                             |
| `eth_getCode`                             |                                                                             |
| `eth_getStorageAt`                        |                                                                             |
| `eth_getTransactionCount`                 |                                                                             |
| `eth_getTransactionByHash`                |                                                                             |
| `eth_getTransactionByBlockHashAndIndex`   |                                                                             |
| `eth_getTransactionByBlockNumberAndIndex` |                                                                             |
| `eth_getTransactionReceipt`               |                                                                             |
| `eth_protocolVersion`                     |                                                                             |
| `eth_sendRawTransaction`                  |                                                                             |
| `eth_syncing`                             | Node is considered synced if it's less than 11 blocks behind the main node. |
| `eth_coinbase`                            | Always returns a zero address                                               |
| `eth_accounts`                            | Always returns an empty list                                                |
| `eth_getCompilers`                        | Always returns an empty list                                                |
| `eth_hashrate`                            | Always returns zero                                                         |
| `eth_getUncleCountByBlockHash`            | Always returns zero                                                         |
| `eth_getUncleCountByBlockNumber`          | Always returns zero                                                         |
| `eth_mining`                              | Always returns false                                                        |

### PubSub

Only available on the WebSocket servers.

Available methods:

| Method             | Notes                                           |
| ------------------ | ----------------------------------------------- |
| `eth_subscribe`    | Maximum amount of subscriptions is configurable |
| `eth_subscription` |                                                 |

### `net` namespace

Available methods:

| Method           | Notes                |
| ---------------- | -------------------- |
| `net_version`    |                      |
| `net_peer_count` | Always returns 0     |
| `net_listening`  | Always returns false |

### `web3` namespace

Available methods:

| Method               | Notes |
| -------------------- | ----- |
| `web3_clientVersion` |       |

### `debug` namespace

The `debug` namespace gives access to several non-standard RPC methods, which will allow developers to inspect and debug
calls and transactions.

This namespace is disabled by default and can be configured via setting `EN_API_NAMESPACES` as described in the
[example config](prepared_configs/mainnet-config.env).

```admonish note
The traces will only start being generated for blocks synced after the debug namespace is enabled, they will not be
backfilled! The only way to get traces for historical blocks is to fully re-sync the node
```

Available methods:

| Method                     | Notes |
| -------------------------- | ----- |
| `debug_traceBlockByNumber` |       |
| `debug_traceBlockByHash`   |       |
| `debug_traceCall`          |       |
| `debug_traceTransaction`   |       |

### `zks` namespace

This namespace contains rollup-specific extensions to the Web3 API. Note that _only methods_ specified in the
[documentation][zks_docs] are considered public. There may be other methods exposed in this namespace, but undocumented
methods come without any kind of stability guarantees and can be changed or removed without notice.

Always refer to the documentation linked above to see the list of stabilized methods in this namespace.

[zks_docs]: https://docs.zksync.io/build/api-reference/zks-rpc

### `en` namespace

This namespace contains methods that Nodes call on the main node while syncing. If this namespace is enabled, other ENs
can sync from this node.
