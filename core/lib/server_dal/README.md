# DAL (Data access Layer)

This crate provides read and write access to the main database (which is Postgres), that acts as a primary source of
truth.

Current schema is managed by `diesel` - that applies all the schema changes from `migrations` directory.

## Schema

### Storage tables

| Table name   | Description                                                                       | Usage                                                                                                                                                     |
| ------------ | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| storage      | Main storage column: mapping from hashed StorageKey (account + key) to the value. | We also store additional columns there (like transaction hash or creation time).                                                                          |
| storage_logs | Stores all the storage access logs for all the transactions.                      | Main source of truth - other columns (like `storage`) are created by compacting this column. Its primary index is (storage key, mini_block, operation_id) |

### Prover queue tables

The tables below are used by different parts of witness generation.

| Table name                    | Description                        |
| ----------------------------- | ---------------------------------- |
| witness_inputs                | TODO                               |
| leaf_aggregation_witness_jobs | Queue of jobs for leaf aggregation |
| node_aggregation_witness_jobs | Queue of jobs for node aggregation |
| scheduler_witness_jobs        | TODO                               |

### TODO

| Table name                            |
| ------------------------------------- |
| \_sqlx_migrations                     |
| aggregated_proof                      |
| contract_verification_requests        |
| contract_verification_solc_versions   |
| contract_verification_zksolc_versions |
| contracts_verification_info           |
| eth_txs                               |
| eth_txs_history                       |
| events                                |
| factory_deps                          |
| gpu_prover_queue                      |
| initial_writes                        |
| l1_batches                            |
| l2_to_l1_logs                         |
| miniblocks                            |
| proof                                 |
| protective_reads                      |
| prover_jobs                           |
| static_artifact_storage               |
| storage_logs_dedup                    |
| tokens                                |
| transaction_traces                    |
| transactions                          |
