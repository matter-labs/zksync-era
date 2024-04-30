use std::{path::Path, sync::Arc, time::Duration};

use anyhow::Context as _;
use serde::Serialize;
use tokio::fs;
use zksync_config::{configs::chain::NetworkConfig, ContractsConfig, EthConfig};
use zksync_contracts::hyperchain_contract;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_signer::{EthereumSigner, PrivateKeySigner, TransactionParameters};
use zksync_merkle_tree::domain::ZkSyncTree;
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_state::RocksdbStorage;
use zksync_storage::RocksDB;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    ethabi::Token,
    snapshots::{
        SnapshotFactoryDependencies, SnapshotMetadata, SnapshotStorageLogsChunk,
        SnapshotStorageLogsStorageKey,
    },
    web3::{
        contract::{Contract, Options},
        transports::Http,
        types::{BlockId, BlockNumber},
        Web3,
    },
    Address, K256PrivateKey, L1BatchNumber, L2ChainId, H160, H256, U256,
};

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct BlockReverterEthConfig {
    eth_client_url: String,
    reverter_private_key: Option<K256PrivateKey>,
    diamond_proxy_addr: H160,
    validator_timelock_addr: H160,
    default_priority_fee_per_gas: u64,
    hyperchain_id: L2ChainId,
    era_chain_id: L2ChainId,
}

impl BlockReverterEthConfig {
    pub fn new(
        eth_config: EthConfig,
        contract: &ContractsConfig,
        network_config: &NetworkConfig,
        era_chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        #[allow(deprecated)]
        // `BlockReverter` doesn't support non env configs yet
        let reverter_private_key = eth_config
            .sender
            .context("eth_sender_config")?
            .private_key()
            .context("eth_sender_config.private_key")?;

        Ok(Self {
            eth_client_url: eth_config.web3_url,
            reverter_private_key,
            diamond_proxy_addr: contract.diamond_proxy_addr,
            validator_timelock_addr: contract.validator_timelock_addr,
            default_priority_fee_per_gas: eth_config
                .gas_adjuster
                .context("gas adjuster")?
                .default_priority_fee_per_gas,
            hyperchain_id: network_config.zksync_network_id,
            era_chain_id,
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
/// - State of the state keeper cache
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
    state_keeper_cache_path: Option<String>,
    merkle_tree_path: Option<String>,
    snapshots_object_store: Option<Arc<dyn ObjectStore>>,
}

impl BlockReverter {
    pub fn new(node_role: NodeRole, connection_pool: ConnectionPool<Core>) -> Self {
        Self {
            node_role,
            allow_rolling_back_executed_batches: false,
            connection_pool,
            should_roll_back_postgres: false,
            state_keeper_cache_path: None,
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

    pub fn enable_rolling_back_merkle_tree(&mut self, path: String) -> &mut Self {
        self.merkle_tree_path = Some(path);
        self
    }

    pub fn enable_rolling_back_state_keeper_cache(&mut self, path: String) -> &mut Self {
        self.state_keeper_cache_path = Some(path);
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
    pub async fn roll_back(&self, last_l1_batch_to_keep: L1BatchNumber) -> anyhow::Result<()> {
        if !self.allow_rolling_back_executed_batches {
            let mut storage = self.connection_pool.connection().await?;
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

        if let Some(object_store) = &self.snapshots_object_store {
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
            let storage_root_hash = self
                .connection_pool
                .connection()
                .await?
                .blocks_dal()
                .get_l1_batch_state_root(last_l1_batch_to_keep)
                .await?
                .context("no state root hash for target L1 batch")?;

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

        if let Some(state_keeper_cache_path) = &self.state_keeper_cache_path {
            let sk_cache_exists = fs::try_exists(state_keeper_cache_path)
                .await
                .with_context(|| {
                    format!(
                        "cannot check whether state keeper cache path `{state_keeper_cache_path}` exists"
                    )
                })?;
            anyhow::ensure!(
                sk_cache_exists,
                "Path with state keeper cache DB doesn't exist at `{state_keeper_cache_path}`"
            );
            self.roll_back_state_keeper_cache(last_l1_batch_to_keep, state_keeper_cache_path)
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
        let mut tree = ZkSyncTree::new_lightweight(db.into());

        if tree.next_l1_batch_number() <= last_l1_batch_to_keep {
            tracing::info!("Tree is behind the L1 batch to roll back to; skipping");
            return Ok(());
        }
        tree.roll_back_logs(last_l1_batch_to_keep);

        tracing::info!("Checking match of the tree root hash and root hash from Postgres");
        let tree_root_hash = tree.root_hash();
        anyhow::ensure!(
            tree_root_hash == storage_root_hash,
            "Mismatch between the tree root hash {tree_root_hash:?} and storage root hash {storage_root_hash:?} after rollback"
        );
        tracing::info!("Saving tree changes to disk");
        tree.save();
        Ok(())
    }

    /// Rolls back changes in the state keeper cache.
    async fn roll_back_state_keeper_cache(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
        state_keeper_cache_path: &str,
    ) -> anyhow::Result<()> {
        tracing::info!("Opening DB with state keeper cache at `{state_keeper_cache_path}`");
        let sk_cache = RocksdbStorage::builder(state_keeper_cache_path.as_ref())
            .await
            .context("failed initializing state keeper cache")?;

        if sk_cache.l1_batch_number().await > Some(last_l1_batch_to_keep + 1) {
            let mut storage = self.connection_pool.connection().await?;
            tracing::info!("Rolling back state keeper cache");
            sk_cache
                .roll_back(&mut storage, last_l1_batch_to_keep)
                .await
                .context("failed rolling back state keeper cache")?;
        } else {
            tracing::info!("Nothing to roll back in state keeper cache");
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
        let mut storage = self.connection_pool.connection().await?;
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
        transaction
            .blocks_dal()
            .delete_initial_writes(last_l1_batch_to_keep)
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

            for chunk_id in 0..snapshot.storage_logs_filepaths.len() as u64 {
                let key = SnapshotStorageLogsStorageKey {
                    l1_batch_number: snapshot.l1_batch_number,
                    chunk_id,
                };
                tracing::info!("Removing storage logs chunk {key:?}");

                let result = object_store
                    .remove::<SnapshotStorageLogsChunk>(key)
                    .await
                    .or_else(ignore_not_found_errors)
                    .with_context(|| format!("failed removing storage logs chunk {key:?}"));
                combine_results(&mut overall_result, result);
            }
        }
        overall_result
    }

    /// Sends a revert transaction to L1.
    pub async fn send_ethereum_revert_transaction(
        &self,
        eth_config: &BlockReverterEthConfig,
        last_l1_batch_to_keep: L1BatchNumber,
        priority_fee_per_gas: U256,
        nonce: u64,
    ) -> anyhow::Result<()> {
        let web3 =
            Web3::new(Http::new(&eth_config.eth_client_url).context("cannot create L1 client")?);
        let contract = hyperchain_contract();
        let signer = PrivateKeySigner::new(
            eth_config
                .reverter_private_key
                .clone()
                .context("private key is required to send revert transaction")?,
        );
        let chain_id = web3
            .eth()
            .chain_id()
            .await
            .context("failed getting L1 chain ID")?
            .as_u64();

        // It is expected that for all new chains `revertBatchesSharedBridge` can be used.
        // For Era, we are using `revertBatches` function for backwards compatibility in case the migration
        // to the shared bridge is not yet complete.
        let data = if eth_config.hyperchain_id == eth_config.era_chain_id {
            let revert_function = contract
                .function("revertBatches")
                .context("`revertBatches` function must be present in contract")?;
            revert_function
                .encode_input(&[Token::Uint(last_l1_batch_to_keep.0.into())])
                .context("failed encoding `revertBatches` input")?
        } else {
            let revert_function = contract
                .function("revertBatchesSharedBridge")
                .context("`revertBatchesSharedBridge` function must be present in contract")?;
            revert_function
                .encode_input(&[
                    Token::Uint(eth_config.hyperchain_id.as_u64().into()),
                    Token::Uint(last_l1_batch_to_keep.0.into()),
                ])
                .context("failed encoding `revertBatchesSharedBridge` input")?
        };

        let base_fee = web3
            .eth()
            .block(BlockId::Number(BlockNumber::Pending))
            .await
            .context("failed getting pending L1 block")?
            .map(|block| {
                block
                    .base_fee_per_gas
                    .context("no base_fee_per_gas in pending block")
            })
            .transpose()?;
        let base_fee = if let Some(base_fee) = base_fee {
            base_fee
        } else {
            // Pending block doesn't exist, use the latest one.
            web3.eth()
                .block(BlockId::Number(BlockNumber::Latest))
                .await
                .context("failed geting latest L1 block")?
                .context("no latest L1 block")?
                .base_fee_per_gas
                .context("no base_fee_per_gas in latest block")?
        };

        let tx = TransactionParameters {
            to: eth_config.validator_timelock_addr.into(),
            data,
            chain_id,
            nonce: nonce.into(),
            max_priority_fee_per_gas: priority_fee_per_gas,
            max_fee_per_gas: base_fee + priority_fee_per_gas,
            gas: 5_000_000.into(),
            ..Default::default()
        };

        let signed_tx = signer
            .sign_transaction(tx)
            .await
            .context("cannot sign revert transaction")?;
        let hash = web3
            .eth()
            .send_raw_transaction(signed_tx.into())
            .await
            .context("failed sending revert transaction")?;
        tracing::info!("Sent revert transaction to L1 with hash {hash:?}");

        loop {
            let maybe_receipt = web3
                .eth()
                .transaction_receipt(hash)
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

    #[tracing::instrument(skip(contract), err, fields(contract.address = ?contract.address()))]
    async fn get_l1_batch_number_from_contract(
        contract: &Contract<Http>,
        op: AggregatedActionType,
    ) -> anyhow::Result<L1BatchNumber> {
        let function_name = match op {
            AggregatedActionType::Commit => "getTotalBatchesCommitted",
            AggregatedActionType::PublishProofOnchain => "getTotalBatchesVerified",
            AggregatedActionType::Execute => "getTotalBatchesExecuted",
        };
        let block_number: U256 = contract
            .query(function_name, (), None, Options::default(), None)
            .await
            .with_context(|| {
                format!(
                    "failed calling `{function_name}` for contract {:?}",
                    contract.address()
                )
            })?;
        Ok(L1BatchNumber(block_number.as_u32()))
    }

    /// Returns suggested values for a reversion.
    pub async fn suggested_values(
        &self,
        eth_config: &BlockReverterEthConfig,
        reverter_address: Address,
    ) -> anyhow::Result<SuggestedRevertValues> {
        let web3 =
            Web3::new(Http::new(&eth_config.eth_client_url).context("cannot create L1 client")?);
        let contract_address = eth_config.diamond_proxy_addr;
        let contract = Contract::new(web3.eth(), contract_address, hyperchain_contract());

        let last_committed_l1_batch_number =
            Self::get_l1_batch_number_from_contract(&contract, AggregatedActionType::Commit)
                .await?;
        let last_verified_l1_batch_number = Self::get_l1_batch_number_from_contract(
            &contract,
            AggregatedActionType::PublishProofOnchain,
        )
        .await?;
        let last_executed_l1_batch_number =
            Self::get_l1_batch_number_from_contract(&contract, AggregatedActionType::Execute)
                .await?;

        tracing::info!(
            "Last L1 batch numbers on contract: committed {last_committed_l1_batch_number}, \
             verified {last_verified_l1_batch_number}, executed {last_executed_l1_batch_number}"
        );

        let priority_fee = eth_config.default_priority_fee_per_gas;
        let nonce = web3
            .eth()
            .transaction_count(reverter_address, Some(BlockNumber::Pending))
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
            .connection()
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
