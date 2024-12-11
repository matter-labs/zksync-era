# Steps to modify the docker-compose files to support Other Chains

Below are the steps for adjusting ZKsync Era docker-compose files from [here](00_quick_start.md) to support chains other
than ZKsync Era.

> [!NOTE]
>
> If you want to run Node for a given chain, you can first ask the company hosting the chains for the Dockerfiles.

## 1. Update `EN_L2_CHAIN_ID`

The `EN_L2_CHAIN_ID` environment variable specifies the Layer 2 chain ID of the blockchain.

You can get it using main node rpc call `eth_chainId` or by asking the company hosting the chain. For example:

```
curl -X POST https://mainnet.era.zksync.io \
-H "Content-Type: application/json" \
-d '{"jsonrpc": "2.0", "method": "eth_chainId", "params": [], "id": 1}'
```

returns

```
{ "jsonrpc": "2.0", "result": "0x144", "id": 1}
```

where `0x144` is the chain ID (324 in decimal)

## 2. Update `EN_MAIN_NODE_URL`

The `EN_MAIN_NODE_URL` The EN_MAIN_NODE_URL environment variable should point to the main node URL of the target chain

## 3. Update snapshots recovery settings

Snapshots recovery is a feature that allows faster Node startup at the cost of no transaction history. By default the
ZKsync Era docker-compose file has this feature enabled, but it's only recommended to use if the Node first startup time
is too slow. It can be disabled by changing `EN_SNAPSHOTS_RECOVERY_ENABLED` to `false`

If you want to keep this feature enabled for a Node, ask the company hosting the chain for the bucket name where the
snapshots are stored and update the value of `EN_SNAPSHOTS_OBJECT_STORE_BUCKET_BASE_URL`

## 4. Disable consensus

Chains other than ZKsync Era aren't currently running consensus(as of December 2024). You need to disable it by removing
`--enable-consensus` flag from `entrypoint.sh` invocation in docker-compose

## 5. (Validium chains only) Set `EN_L1_BATCH_COMMIT_DATA_GENERATOR_MODE`

For validium chains, you need to set `EN_L1_BATCH_COMMIT_DATA_GENERATOR_MODE: "Validium"`
