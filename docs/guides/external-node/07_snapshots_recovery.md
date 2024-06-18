# Snapshots Recovery

Instead of starting node using DB snapshots, it's possible to configure them to start from a protocol-level snapshots.
This process is much faster and requires way less storage. Postgres database of a mainnet node recovered from a snapshot
is only about 300GB.

> [!NOTE] 
> 
> Nodes recovered from snapshot don't have any historical data from before the recovery!

## Configuration

To enable snapshots-recovery on mainnet, you need to set:

      EN_SNAPSHOTS_RECOVERY_ENABLED: "true"
      EN_SNAPSHOTS_OBJECT_STORE_BUCKET_BASE_URL: "zksync-era-mainnet-external-node-snapshots"
      EN_SNAPSHOTS_OBJECT_STORE_MODE: "GCSAnonymousReadOnly"

For sepolia testnet, use:

      EN_SNAPSHOTS_RECOVERY_ENABLED: "true"
      EN_SNAPSHOTS_OBJECT_STORE_BUCKET_BASE_URL: "zksync-era-boojnet-external-node-snapshots"
      EN_SNAPSHOTS_OBJECT_STORE_MODE: "GCSAnonymousReadOnly"

For a working examples of a fully configured Nodes recovering from snapshots, see docker-compose-examples directory and
00_quick_start.md
