# State Keeper State

Access to the VM storage for the state keeper. The state keeper itself is a part of the [`zksync_core`] crate; it is a
component responsible for handling transaction execution and creating miniblocks and L1 batches.

All state keeper data is currently stored in Postgres. (Beside it, we provide an in-memory implementation for
benchmarking / testing purposes.) We also keep a secondary copy for part of it in RocksDB for performance reasons.
Currently, we only duplicate the data needed by the [`vm`] crate.

[`zksync_core`]: ../zksync_core
[`vm`]: ../vm
