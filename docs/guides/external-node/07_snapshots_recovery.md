# Snapshots Recovery

Instead of starting node using DB snapshots, it's possible to configure them to start from a protocol-level snapshots.
This process is much faster and requires way less storage. Postgres database of a mainnet node recovered from a snapshot
is only about 300GB. Without [_pruning_](08_pruning.md) enabled, the state will continuously grow about 15GB per day.

> [!NOTE]
>
> Nodes recovered from snapshot don't have any historical data from before the recovery!

## Configuration

To enable snapshots-recovery on mainnet, you need to set environment variables:

```yaml
EN_SNAPSHOTS_RECOVERY_ENABLED: 'true'
EN_SNAPSHOTS_OBJECT_STORE_BUCKET_BASE_URL: 'zksync-era-mainnet-external-node-snapshots'
EN_SNAPSHOTS_OBJECT_STORE_MODE: 'GCSAnonymousReadOnly'
```

For sepolia testnet, use:

```yaml
EN_SNAPSHOTS_RECOVERY_ENABLED: 'true'
EN_SNAPSHOTS_OBJECT_STORE_BUCKET_BASE_URL: 'zksync-era-boojnet-external-node-snapshots'
EN_SNAPSHOTS_OBJECT_STORE_MODE: 'GCSAnonymousReadOnly'
```

For a working examples of a fully configured Nodes recovering from snapshots, see
[_docker compose examples_](docker-compose-examples) directory and [_Quick Start_](00_quick_start.md)
