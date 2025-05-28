use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::Context as _;
use serde::Serialize;
use tokio::{fs, sync::Semaphore};
use zksync_config::EthConfig;
use zksync_contracts::hyperchain_contract;
use zksync_dal::{ConnectionPool, Core, CoreDal};
// Public re-export to simplify the API use.
pub use zksync_eth_client as eth_client;
use zksync_eth_client::{BoundEthInterface, CallFunctionArgs, EthInterface, Options};
use zksync_merkle_tree::domain::ZkSyncTree;
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_state::RocksdbStorage;
use zksync_storage::RocksDB;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    ethabi::Token,
    settlement::SettlementLayer,
    snapshots::{
        SnapshotFactoryDependencies, SnapshotMetadata, SnapshotStorageLogsChunk,
        SnapshotStorageLogsStorageKey,
    },
    web3::BlockNumber,
    Address, L1BatchNumber, L2ChainId, H160, H256, U256,
};

pub mod node;
#[cfg(test)]
mod tests;

/// The amount of gas to be used for transactions on top of Gateway.
/// It is larger than the L1 one, since the gas required is typically
/// higher on gateway, due to pubdata price fluctuations.
const GATEWAY_DEFAULT_GAS: usize = 50_000_000;
/// The amount of gas to be used for transactions on top of L1 chains.
const L1_DEFAULT_GAS: usize = 5_000_000;

#[derive(Debug)]
pub struct BlockReverterEthConfig {
    sl_diamond_proxy_addr: H160,
    sl_validator_timelock_addr: H160,
    default_priority_fee_per_gas: u64,
    hyperchain_id: L2ChainId,
    settlement_layer: SettlementLayer,
}

impl BlockReverterEthConfig {
    pub fn new(
        eth_config: &EthConfig,
        sl_diamond_proxy_addr: Address,
        sl_validator_timelock_addr: Address,
        hyperchain_id: L2ChainId,
        settlement_layer: SettlementLayer,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            sl_diamond_proxy_addr,
            sl_validator_timelock_addr,
            default_priority_fee_per_gas: eth_config.gas_adjuster.default_priority_fee_per_gas,
            hyperchain_id,
            settlement_layer,
        })
    }
}

/// Role of the node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeRole {
    Main,
    External,
}

/// This struct is used to roll back node state and revert batches committed (but generally not finalized) on L1.
///
/// Reversion is a rare event of manual intervention, when the node operator
/// decides to revert some of the not yet finalized batches for some reason
/// (e.g. inability to generate a proof).
///
/// It is also used to automatically roll back the external node state
/// after it detects a reorg on the main node. Notice the difference in terminology; from the network
/// perspective, *reversion* leaves a trace (L1 transactions etc.), while from the perspective of the node state, a *rollback*
/// leaves the node in the same state as if the rolled back batches were never sealed in the first place.
///
/// `BlockReverter` can roll back the following pieces of node state:
///
/// - State of the Postgres database
/// - State of the Merkle tree
/// - State of the RocksDB storage cache
/// - Object store for protocol snapshots
///
/// In addition, it can revert the state of the Ethereum contract (if the reverted L1 batches were committed).
#[derive(Debug)]
pub struct BlockReverter {
    /// Role affects the interactions with the consensus state.
    /// This distinction will be removed once consensus genesis is moved to the L1 state.
    node_role: NodeRole,
    allow_rolling_back_executed_batches: bool,
    connection_pool: ConnectionPool<Core>,
    should_roll_back_postgres: bool,
    storage_cache_paths: Vec<PathBuf>,
    merkle_tree_path: Option<PathBuf>,
    snapshots_object_store: Option<Arc<dyn ObjectStore>>,
}

impl BlockReverter {
    pub fn new(node_role: NodeRole, connection_pool: ConnectionPool<Core>) -> Self {
        Self {
            node_role,
            allow_rolling_back_executed_batches: false,
            connection_pool,
            should_roll_back_postgres: false,
            storage_cache_paths: Vec::new(),
            merkle_tree_path: None,
            snapshots_object_store: None,
        }
    }

    /// Allows rolling back the state past the last batch finalized on L1. If this is disallowed (which is the default),
    /// block reverter will error upon such an attempt.
    ///
    /// Main use case for the setting this flag is the external node, where may obtain an
    /// incorrect state even for a block that was marked as executed. On the EN, this mode is not destructive.
    pub fn allow_rolling_back_executed_batches(&mut self) -> &mut Self {
        self.allow_rolling_back_executed_batches = true;
        self
    }

    pub fn enable_rolling_back_postgres(&mut self) -> &mut Self {
        self.should_roll_back_postgres = true;
        self
    }

    pub fn enable_rolling_back_merkle_tree(&mut self, path: PathBuf) -> &mut Self {
        self.merkle_tree_path = Some(path);
        self
    }

    pub fn add_rocksdb_storage_path_to_rollback(&mut self, path: PathBuf) -> &mut Self {
        self.storage_cache_paths.push(path);
        self
    }

    pub fn enable_rolling_back_snapshot_objects(
        &mut self,
        object_store: Arc<dyn ObjectStore>,
    ) -> &mut Self {
        self.snapshots_object_store = Some(object_store);
        self
    }

    /// Rolls back previously enabled DBs (Postgres + RocksDB) and the snapshot object store to a previous state.
    #[tracing::instrument(skip(self), err)]
    pub async fn roll_back(&self, last_l1_batch_to_keep: L1BatchNumber) -> anyhow::Result<()> {
        if !self.allow_rolling_back_executed_batches {
            let mut storage = self
                .connection_pool
                .connection_tagged("block_reverter")
                .await?;
            let last_executed_l1_batch = storage
                .blocks_dal()
                .get_number_of_last_l1_batch_executed_on_eth()
                .await?;
            anyhow::ensure!(
                Some(last_l1_batch_to_keep) >= last_executed_l1_batch,
                "Attempt to roll back already executed L1 batches; the last executed batch is: {last_executed_l1_batch:?}"
            );
        }

        // Tree needs to be rolled back first to keep the state recoverable
        self.roll_back_rocksdb_instances(last_l1_batch_to_keep)
            .await?;
        let deleted_snapshots = if self.should_roll_back_postgres {
            self.roll_back_postgres(last_l1_batch_to_keep).await?
        } else {
            vec![]
        };

        if let Some(object_store) = self.snapshots_object_store.as_deref() {
            Self::delete_snapshot_files(object_store, &deleted_snapshots).await?;
        } else if !deleted_snapshots.is_empty() {
            tracing::info!(
                "Did not remove snapshot files in object store since it was not provided; \
                 metadata for deleted snapshots: {deleted_snapshots:?}"
            );
        }

        Ok(())
    }

    async fn roll_back_rocksdb_instances(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
    ) -> anyhow::Result<()> {
        if let Some(merkle_tree_path) = &self.merkle_tree_path {
            let mut connection = self
                .connection_pool
                .connection_tagged("block_reverter")
                .await?;
            let storage_root_hash = connection
                .blocks_dal()
                .get_l1_batch_state_root(last_l1_batch_to_keep)
                .await?;
            let Some(storage_root_hash) = storage_root_hash else {
                let latest_l1_batch = connection.blocks_dal().get_sealed_l1_batch_number().await?;
                let earliest_l1_batch = connection
                    .blocks_dal()
                    .get_earliest_l1_batch_number()
                    .await?;
                anyhow::bail!(
                    "no state root hash for target L1 batch #{last_l1_batch_to_keep}; \
                     Postgres contains batches from {earliest_l1_batch:?} to {latest_l1_batch:?} (both inclusive)"
                );
            };
            drop(connection);

            let merkle_tree_path = Path::new(merkle_tree_path);
            let merkle_tree_exists = fs::try_exists(merkle_tree_path).await.with_context(|| {
                format!(
                    "cannot check whether Merkle tree path `{}` exists",
                    merkle_tree_path.display()
                )
            })?;
            if merkle_tree_exists {
                tracing::info!(
                    "Rolling back Merkle tree at `{}`",
                    merkle_tree_path.display()
                );
                let merkle_tree_path = merkle_tree_path.to_path_buf();
                tokio::task::spawn_blocking(move || {
                    Self::roll_back_tree_blocking(
                        last_l1_batch_to_keep,
                        &merkle_tree_path,
                        storage_root_hash,
                    )
                })
                .await
                .context("rolling back Merkle tree panicked")??;
            } else {
                tracing::info!(
                    "Merkle tree not found at `{}`; skipping",
                    merkle_tree_path.display()
                );
            }
        }

        for storage_cache_path in &self.storage_cache_paths {
            let sk_cache_exists = fs::try_exists(storage_cache_path).await.with_context(|| {
                format!("cannot check whether storage cache path `{storage_cache_path:?}` exists")
            })?;
            if !sk_cache_exists {
                tracing::info!("Storage cache doesn't exist at `{storage_cache_path:?}`; skipping");
                continue;
            }
            self.roll_back_storage_cache(last_l1_batch_to_keep, storage_cache_path)
                .await?;
        }
        Ok(())
    }

    fn roll_back_tree_blocking(
        last_l1_batch_to_keep: L1BatchNumber,
        path: &Path,
        storage_root_hash: H256,
    ) -> anyhow::Result<()> {
        let db = RocksDB::new(path).context("failed initializing RocksDB for Merkle tree")?;
        let mut tree =
            ZkSyncTree::new_lightweight(db.into()).context("failed initializing Merkle tree")?;

        if tree.next_l1_batch_number() <= last_l1_batch_to_keep {
            tracing::info!("Tree is behind the L1 batch to roll back to; skipping");
            return Ok(());
        }
        tree.roll_back_logs(last_l1_batch_to_keep)
            .context("cannot roll back Merkle tree")?;

        tracing::info!("Checking match of the tree root hash and root hash from Postgres");
        let tree_root_hash = tree.root_hash();
        anyhow::ensure!(
            tree_root_hash == storage_root_hash,
            "Mismatch between the tree root hash {tree_root_hash:?} and storage root hash {storage_root_hash:?} after rollback"
        );
        tracing::info!("Saving tree changes to disk");
        tree.save().context("failed saving tree changes")?;
        Ok(())
    }

    /// Rolls back changes in the storage cache.
    async fn roll_back_storage_cache(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
        storage_cache_path: &Path,
    ) -> anyhow::Result<()> {
        tracing::info!("Opening DB with storage cache at `{storage_cache_path:?}`");
        let sk_cache = RocksdbStorage::builder(storage_cache_path)
            .await
            .context("failed initializing storage cache")?;
        let Some(mut sk_cache) = sk_cache.get().await else {
            tracing::info!(
                "Storage cache at `{storage_cache_path:?}` is not initialized; nothing to do"
            );
            return Ok(());
        };

        if sk_cache.next_l1_batch_number().await > last_l1_batch_to_keep + 1 {
            let mut storage = self
                .connection_pool
                .connection_tagged("block_reverter")
                .await?;
            tracing::info!("Rolling back storage cache");
            sk_cache
                .roll_back(&mut storage, last_l1_batch_to_keep)
                .await
                .context("failed rolling back storage cache")?;
        } else {
            tracing::info!("Nothing to roll back in storage cache");
        }
        Ok(())
    }

    /// Rolls back data in the Postgres database.
    /// If `node_role` is `Main` a consensus hard-fork is performed.
    async fn roll_back_postgres(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
    ) -> anyhow::Result<Vec<SnapshotMetadata>> {
        tracing::info!("Rolling back Postgres data");
        let mut storage = self
            .connection_pool
            .connection_tagged("block_reverter")
            .await?;
        let mut transaction = storage.start_transaction().await?;

        let (_, last_l2_block_to_keep) = transaction
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(last_l1_batch_to_keep)
            .await?
            .with_context(|| {
                format!("L1 batch #{last_l1_batch_to_keep} doesn't contain L2 blocks")
            })?;

        tracing::info!("Rolling back transactions state");
        transaction
            .transactions_dal()
            .reset_transactions_state(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back events");
        transaction
            .events_dal()
            .roll_back_events(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back L2-to-L1 logs");
        transaction
            .events_dal()
            .roll_back_l2_to_l1_logs(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back created tokens");
        transaction
            .tokens_dal()
            .roll_back_tokens(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back factory deps");
        transaction
            .factory_deps_dal()
            .roll_back_factory_deps(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back storage logs");
        transaction
            .storage_logs_dal()
            .roll_back_storage_logs(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back Ethereum transactions");
        transaction
            .eth_sender_dal()
            .delete_eth_txs(last_l1_batch_to_keep)
            .await?;

        tracing::info!("Rolling back snapshots");
        let deleted_snapshots = transaction
            .snapshots_dal()
            .delete_snapshots_after(last_l1_batch_to_keep)
            .await?;

        // Remove data from main tables (L2 blocks and L1 batches).
        tracing::info!("Rolling back L1 batches");
        transaction
            .blocks_dal()
            .delete_l1_batches(last_l1_batch_to_keep)
            .await?;
        tracing::info!("Rolling back initial writes");
        transaction
            .blocks_dal()
            .delete_initial_writes(last_l1_batch_to_keep)
            .await?;
        tracing::info!("Rolling back vm_runner_protective_reads");
        transaction
            .vm_runner_dal()
            .delete_protective_reads(last_l1_batch_to_keep)
            .await?;
        tracing::info!("Rolling back vm_runner_bwip");
        transaction
            .vm_runner_dal()
            .delete_bwip_data(last_l1_batch_to_keep)
            .await?;
        tracing::info!("Rolling back L2 blocks");
        transaction
            .blocks_dal()
            .delete_l2_blocks(last_l2_block_to_keep)
            .await?;

        if self.node_role == NodeRole::Main {
            tracing::info!("Performing consensus hard fork");
            transaction.consensus_dal().fork().await?;
        }

        transaction.commit().await?;
        Ok(deleted_snapshots)
    }

    async fn delete_snapshot_files(
        object_store: &dyn ObjectStore,
        deleted_snapshots: &[SnapshotMetadata],
    ) -> anyhow::Result<()> {
        const CONCURRENT_REMOVE_REQUESTS: usize = 20;

        fn ignore_not_found_errors(err: ObjectStoreError) -> Result<(), ObjectStoreError> {
            match err {
                ObjectStoreError::KeyNotFound(err) => {
                    tracing::debug!("Ignoring 'not found' object store error: {err}");
                    Ok(())
                }
                _ => Err(err),
            }
        }

        fn combine_results(output: &mut anyhow::Result<()>, result: anyhow::Result<()>) {
            if let Err(err) = result {
                tracing::warn!("{err:?}");
                *output = Err(err);
            }
        }

        if deleted_snapshots.is_empty() {
            return Ok(());
        }

        let mut overall_result = Ok(());
        for snapshot in deleted_snapshots {
            tracing::info!(
                "Removing factory deps for snapshot for L1 batch #{}",
                snapshot.l1_batch_number
            );
            let result = object_store
                .remove::<SnapshotFactoryDependencies>(snapshot.l1_batch_number)
                .await
                .or_else(ignore_not_found_errors)
                .with_context(|| {
                    format!(
                        "failed removing factory deps for snapshot for L1 batch #{}",
                        snapshot.l1_batch_number
                    )
                });
            combine_results(&mut overall_result, result);

            let mut is_incomplete_snapshot = false;
            let chunk_ids_iter = (0_u64..)
                .zip(&snapshot.storage_logs_filepaths)
                .filter_map(|(chunk_id, path)| {
                    if path.is_none() {
                        if !is_incomplete_snapshot {
                            is_incomplete_snapshot = true;
                            tracing::warn!(
                                "Snapshot for L1 batch #{} is incomplete (misses al least storage logs chunk ID {chunk_id}). \
                                 It is probable that it's currently being created, in which case you'll need to clean up produced files \
                                 manually afterwards",
                                snapshot.l1_batch_number
                            );
                        }
                        return None;
                    }
                    Some(chunk_id)
                });

            let remove_semaphore = &Semaphore::new(CONCURRENT_REMOVE_REQUESTS);
            let remove_futures = chunk_ids_iter.map(|chunk_id| async move {
                let _permit = remove_semaphore
                    .acquire()
                    .await
                    .context("semaphore is never closed")?;

                let key = SnapshotStorageLogsStorageKey {
                    l1_batch_number: snapshot.l1_batch_number,
                    chunk_id,
                };
                tracing::info!("Removing storage logs chunk {key:?}");
                object_store
                    .remove::<SnapshotStorageLogsChunk>(key)
                    .await
                    .or_else(ignore_not_found_errors)
                    .with_context(|| format!("failed removing storage logs chunk {key:?}"))
            });
            let remove_results = futures::future::join_all(remove_futures).await;

            for result in remove_results {
                combine_results(&mut overall_result, result);
            }
        }
        overall_result
    }

    /// Sends a revert transaction to L1.
    pub async fn send_ethereum_revert_transaction(
        &self,
        sl_client: &dyn BoundEthInterface,
        eth_config: &BlockReverterEthConfig,
        last_l1_batch_to_keep: L1BatchNumber,
        nonce: u64,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Sending Ethereum revert transaction for L1 batch #{last_l1_batch_to_keep} with config {eth_config:?}, \
             nonce: {nonce}"
        );

        let contract = hyperchain_contract();

        let revert_function = contract
            .function("revertBatchesSharedBridge")
            .context("`revertBatchesSharedBridge` function must be present in contract")?;
        let data = revert_function
            .encode_input(&[
                Token::Uint(eth_config.hyperchain_id.as_u64().into()),
                Token::Uint(last_l1_batch_to_keep.0.into()),
            ])
            .context("failed encoding `revertBatchesSharedBridge` input")?;

        let gas = if eth_config.settlement_layer.is_gateway() {
            GATEWAY_DEFAULT_GAS
        } else {
            L1_DEFAULT_GAS
        };

        let options = Options {
            nonce: Some(nonce.into()),
            gas: Some(gas.into()),
            ..Default::default()
        };

        let signed_tx = sl_client
            .sign_prepared_tx_for_addr(data, eth_config.sl_validator_timelock_addr, options)
            .await
            .context("cannot sign revert transaction")?;
        let hash = sl_client
            .as_ref()
            .send_raw_tx(signed_tx.raw_tx)
            .await
            .context("failed sending revert transaction")?;
        tracing::info!("Sent revert transaction to L1 with hash {hash:?}");

        loop {
            let maybe_receipt = sl_client
                .as_ref()
                .tx_receipt(hash)
                .await
                .context("failed getting receipt for revert transaction")?;
            if let Some(receipt) = maybe_receipt {
                anyhow::ensure!(
                    receipt.status == Some(1.into()),
                    "Revert transaction {hash:?} failed with status {:?}",
                    receipt.status
                );
                tracing::info!("Revert transaction has completed");
                return Ok(());
            } else {
                tracing::info!("waiting for L1 transaction confirmation...");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }

    #[tracing::instrument(err)]
    async fn get_l1_batch_number_from_contract(
        eth_client: &dyn EthInterface,
        contract_address: Address,
        op: AggregatedActionType,
    ) -> anyhow::Result<L1BatchNumber> {
        let function_name = match op {
            AggregatedActionType::Commit => "getTotalBatchesCommitted",
            AggregatedActionType::PublishProofOnchain => "getTotalBatchesVerified",
            AggregatedActionType::Execute => "getTotalBatchesExecuted",
        };
        let block_number: U256 = CallFunctionArgs::new(function_name, ())
            .for_contract(contract_address, &hyperchain_contract())
            .call(eth_client)
            .await
            .with_context(|| {
                format!("failed calling `{function_name}` for contract {contract_address:?}")
            })?;
        Ok(L1BatchNumber(block_number.as_u32()))
    }

    /// Returns suggested values for a reversion.
    pub async fn suggested_values(
        &self,
        eth_client: &dyn EthInterface,
        eth_config: &BlockReverterEthConfig,
        reverter_address: Address,
    ) -> anyhow::Result<SuggestedRevertValues> {
        tracing::info!("Computing suggested revert values for config {eth_config:?}");

        let contract_address = eth_config.sl_diamond_proxy_addr;

        let last_committed_l1_batch_number = Self::get_l1_batch_number_from_contract(
            eth_client,
            contract_address,
            AggregatedActionType::Commit,
        )
        .await?;
        let last_verified_l1_batch_number = Self::get_l1_batch_number_from_contract(
            eth_client,
            contract_address,
            AggregatedActionType::PublishProofOnchain,
        )
        .await?;
        let last_executed_l1_batch_number = Self::get_l1_batch_number_from_contract(
            eth_client,
            contract_address,
            AggregatedActionType::Execute,
        )
        .await?;

        tracing::info!(
            "Last L1 batch numbers on contract: committed {last_committed_l1_batch_number}, \
             verified {last_verified_l1_batch_number}, executed {last_executed_l1_batch_number}"
        );

        let priority_fee = eth_config.default_priority_fee_per_gas;
        let nonce = eth_client
            .nonce_at_for_account(reverter_address, BlockNumber::Pending)
            .await
            .with_context(|| format!("failed getting transaction count for {reverter_address:?}"))?
            .as_u64();

        Ok(SuggestedRevertValues {
            last_executed_l1_batch_number,
            nonce,
            priority_fee,
        })
    }

    /// Clears failed L1 transactions.
    pub async fn clear_failed_l1_transactions(&self) -> anyhow::Result<()> {
        tracing::info!("Clearing failed L1 transactions");
        self.connection_pool
            .connection_tagged("block_reverter")
            .await?
            .eth_sender_dal()
            .clear_failed_transactions()
            .await?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct SuggestedRevertValues {
    pub last_executed_l1_batch_number: L1BatchNumber,
    pub nonce: u64,
    pub priority_fee: u64,
}
