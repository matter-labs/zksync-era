# Snapshots Recovery

Instead of initializing a node using a Postgres dump, it's possible to configure a node to recover from a protocol-level
snapshot. This process is much faster and requires much less storage. Postgres database of a mainnet node recovered from
a snapshot is less than 500GB. Note that without [pruning](08_pruning.md) enabled, the node state will continuously grow
at a rate about 15GB per day.

## How it works

A snapshot is effectively a point-in-time snapshot of the VM state at the end of a certain L1 batch. Snapshots are
created for the latest L1 batches periodically (roughly twice a day) and are stored in a public GCS bucket.

Recovery from a snapshot consists of several parts.

- **Postgres** recovery is the initial stage. The node API is not functioning during this stage. The stage is expected
  to take about 1 hour on the mainnet.
- **Merkle tree** recovery starts once Postgres is fully recovered. Merkle tree recovery can take about 3 hours on the
  mainnet. Ordinarily, Merkle tree recovery is a blocker for node synchronization; i.e., the node will not process
  blocks newer than the snapshot block until the Merkle tree is recovered. If the [treeless mode](09_treeless_mode.md)
  is enabled, tree recovery is not performed, and the node will start catching up blocks immediately after Postgres
  recovery. This is still true if the tree data fetcher is enabled _together_ with a Merkle tree; tree recovery is
  asynchronous in this case.
- Recovering RocksDB-based **VM state cache** is concurrent with Merkle tree recovery and also depends on Postgres
  recovery. It takes about 1 hour on the mainnet. Unlike Merkle tree recovery, VM state cache is not necessary for node
  operation (the node will get the state from Postgres is if it is absent), although it considerably speeds up VM
  execution.

After Postgres recovery is completed, the node becomes operational, providing Web3 API etc. It still needs some time to
catch up executing blocks after the snapshot (i.e, roughly several hours worth of blocks / transactions). This may take
order of 1–2 hours on the mainnet. In total, recovery process and catch-up thus should take roughly 5–6 hours with a
Merkle tree, or 3–4 hours in the treeless mode / with a tree data fetcher.

## Current limitations

Nodes recovered from snapshot don't have any historical data from before the recovery. There is currently no way to
back-fill this historic data. E.g., if a node has recovered from a snapshot for L1 batch 500,000; then, it will not have
data for L1 batches 499,999, 499,998, etc. The relevant Web3 methods, such as `eth_getBlockByNumber`, will return an
error mentioning the first locally retained block or L1 batch if queried this missing data. The same error messages are
used for [pruning](08_pruning.md) because logically, recovering from a snapshot is equivalent to pruning node storage to
the snapshot L1 batch.

## Configuration (for ZKsync Era)

To enable snapshot recovery on ZKsync mainnet, you need to set environment variables for a node before starting it for
the first time:

```yaml
EN_SNAPSHOTS_RECOVERY_ENABLED: 'true'
EN_SNAPSHOTS_OBJECT_STORE_BUCKET_BASE_URL: 'zksync-era-mainnet-external-node-snapshots'
EN_SNAPSHOTS_OBJECT_STORE_MODE: 'GCSAnonymousReadOnly'
```

For the ZKsync Sepolia testnet, use:

```yaml
EN_SNAPSHOTS_RECOVERY_ENABLED: 'true'
EN_SNAPSHOTS_OBJECT_STORE_BUCKET_BASE_URL: 'zksync-era-boojnet-external-node-snapshots'
EN_SNAPSHOTS_OBJECT_STORE_MODE: 'GCSAnonymousReadOnly'
```

For a working examples of a fully configured ZKsync Nodes recovering from snapshots, see
[Docker Compose examples](https://github.com/matter-labs/zksync-era/tree/main/docs/src/guides/external-node/docker-compose-examples)
and [_Quick Start_](00_quick_start.md).

If a node is already recovered (does not matter whether from a snapshot or from a Postgres dump), setting these env
variables will have no effect; the node will never reset its state.

## Monitoring recovery

Snapshot recovery information is logged with the following targets:

- **Recovery orchestration:** `zksync_external_node::init`
- **Postgres recovery:** `zksync_snapshots_applier`
- **Merkle tree recovery:** `zksync_metadata_calculator::recovery`, `zksync_merkle_tree::recovery`

An example of snapshot recovery logs during the first node start:

```text
2024-06-20T07:25:32.466926Z  INFO zksync_external_node::init: Node has neither genesis L1 batch, nor snapshot recovery info
2024-06-20T07:25:32.466946Z  INFO zksync_external_node::init: Chosen node initialization strategy: SnapshotRecovery
2024-06-20T07:25:32.466951Z  WARN zksync_external_node::init: Proceeding with snapshot recovery. This is an experimental feature; use at your own risk
2024-06-20T07:25:32.475547Z  INFO zksync_snapshots_applier: Found snapshot with data up to L1 batch #7, L2 block #27, version 0, storage logs are divided into 10 chunk(s)
2024-06-20T07:25:32.516142Z  INFO zksync_snapshots_applier: Applied factory dependencies in 27.768291ms
2024-06-20T07:25:32.527363Z  INFO zksync_snapshots_applier: Recovering storage log chunks with 10 max concurrency
2024-06-20T07:25:32.608539Z  INFO zksync_snapshots_applier: Recovered 3007 storage logs in total; checking overall consistency...
2024-06-20T07:25:32.612967Z  INFO zksync_snapshots_applier: Retrieved 2 tokens from main node
2024-06-20T07:25:32.616142Z  INFO zksync_external_node::init: Recovered Postgres from snapshot in 148.523709ms
2024-06-20T07:25:32.645399Z  INFO zksync_metadata_calculator::recovery: Recovering Merkle tree from Postgres snapshot in 1 chunks with max concurrency 10
2024-06-20T07:25:32.650478Z  INFO zksync_metadata_calculator::recovery: Filtered recovered key chunks; 1 / 1 chunks remaining
2024-06-20T07:25:32.681327Z  INFO zksync_metadata_calculator::recovery: Recovered 1/1 Merkle tree chunks, there are 0 left to process
2024-06-20T07:25:32.784597Z  INFO zksync_metadata_calculator::recovery: Recovered Merkle tree from snapshot in 144.040125ms
```

(Obviously, timestamps and numbers in the logs will differ.)

Recovery logic also exports some metrics, the main of which are as follows:

| Metric name                                             | Type  | Labels | Description                                                           |
| ------------------------------------------------------- | ----- | ------ | --------------------------------------------------------------------- |
| `snapshots_applier_storage_logs_chunks_left_to_process` | Gauge | -      | Number of storage log chunks left to process during Postgres recovery |
