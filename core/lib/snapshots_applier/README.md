# `zksync_snapshots_applier`

Library responsible for recovering Postgres from a protocol-level snapshot.

## Recovery workflow

_(See [node docs](../../../docs/src/guides/external-node/07_snapshots_recovery.md) for a high-level snapshot recovery
overview and [snapshot creator docs](../../bin/snapshots_creator/README.md) for the snapshot format details)_

1. Recovery is started by querying the main node and determining the snapshot parameters. By default, recovery is
   performed from the latest snapshot, but it is possible to provide a manual override (L1 batch number of the
   snapshot).
2. Factory dependencies (= contract bytecodes) are downloaded from the object store and are atomically saved to Postgres
   together with the snapshot metadata (L1 batch number / L2 block numbers and timestamps, L1 batch state root hash, L2
   block hash etc.).
3. Storage log chunks are downloaded from the object store; each chunk is atomically saved to Postgres (`storage_logs`
   and `initial_writes` tables). This step has a configurable degree of concurrency to control speed – I/O load
   trade-off.
4. After all storage logs are restored, token information is fetched from the main node and saved in the corresponding
   table. Tokens are double-checked against storage logs.

Recovery is resilient to stops / failures; if the recovery process is interrupted, it will restart from the same
snapshot and will skip saving data that is already present in Postgres.

Recovery logic for node components (such as metadata calculator and state keeper) is intentionally isolated from
Postgres recovery. A component requiring recovery must organize it on its own. This is motivated by the fact that at
least some components requiring recovery may initialize after an arbitrary delay after Postgres recovery (or not run at
all) and/or may be instantiated multiple times for a single node. As an example, both of these requirements hold for
metadata calculator / Merkle tree.
