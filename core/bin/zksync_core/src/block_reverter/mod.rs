use bitflags::bitflags;
use serde::Serialize;
use tokio::time::sleep;

use std::path::Path;
use std::time::Duration;

use zksync_config::ZkSyncConfig;
use zksync_contracts::zksync_contract;
use zksync_dal::ConnectionPool;
use zksync_eth_client::{clients::http::PKSigningClient, BoundEthInterface, EthInterface};
use zksync_merkle_tree::ZkSyncTree;
use zksync_merkle_tree2::domain::ZkSyncTree as NewTree;
use zksync_state::secondary_storage::SecondaryStateStorage;
use zksync_storage::db::Database;
use zksync_storage::RocksDB;
use zksync_types::aggregated_operations::AggregatedActionType;
use zksync_types::ethabi::Token;
use zksync_types::web3::contract::Options;
use zksync_types::{L1BatchNumber, H256, U256};

bitflags! {
    pub struct BlockReverterFlags: u32 {
        const POSTGRES = 0b_0001;
        const TREE = 0b_0010;
        const SK_CACHE = 0b_0100;
    }
}

/// Flag determining whether the reverter is allowed to revert the state
/// past the last batch finalized on L1. If this flag is set to `Disallowed`,
/// block reverter will panic upon such an attempt.
///
/// Main use case for the `Allowed` flag is the external node, where may obtain an
/// incorrect state even for a block that was marked as executed. On the EN, this mode is not destructive.
#[derive(Debug)]
pub enum L1ExecutedBatchesRevert {
    Allowed,
    Disallowed,
}

/// This struct is used to perform a rollback of the state.
/// Rollback is a rare event of manual intervention, when the node operator
/// decides to revert some of the not yet finalized batches for some reason
/// (e.g. inability to generate a proof).
///
/// It is also used to automatically perform a rollback on the external node
/// after it is detected on the main node.
///
/// There are a few state components that we can roll back
/// - State of the Postgres database
/// - State of the merkle tree
/// - State of the state_keeper cache
/// - State of the Ethereum contract (if the block was committed)
#[derive(Debug)]
pub struct BlockReverter {
    config: ZkSyncConfig,
    connection_pool: ConnectionPool,
    executed_batches_revert_mode: L1ExecutedBatchesRevert,
}

impl BlockReverter {
    pub fn new(
        config: ZkSyncConfig,
        connection_pool: ConnectionPool,
        executed_batches_revert_mode: L1ExecutedBatchesRevert,
    ) -> Self {
        Self {
            config,
            connection_pool,
            executed_batches_revert_mode,
        }
    }

    /// Rolls back DBs (Postgres + RocksDB) to a previous state.
    pub async fn rollback_db(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
        flags: BlockReverterFlags,
    ) {
        let rollback_tree = flags.contains(BlockReverterFlags::TREE);
        let rollback_postgres = flags.contains(BlockReverterFlags::POSTGRES);
        let rollback_sk_cache = flags.contains(BlockReverterFlags::SK_CACHE);

        if matches!(
            self.executed_batches_revert_mode,
            L1ExecutedBatchesRevert::Disallowed
        ) {
            let last_executed_l1_batch = self
                .get_l1_batch_number_from_contract(AggregatedActionType::ExecuteBlocks)
                .await;
            assert!(
                last_l1_batch_to_keep >= last_executed_l1_batch,
                "Attempt to revert already executed blocks"
            );
        }

        // Tree needs to be reverted first to keep state recoverable
        self.rollback_rocks_dbs(last_l1_batch_to_keep, rollback_tree, rollback_sk_cache)
            .await;
        if rollback_postgres {
            self.rollback_postgres(last_l1_batch_to_keep);
        }
    }

    async fn rollback_rocks_dbs(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
        rollback_tree: bool,
        rollback_sk_cache: bool,
    ) {
        vlog::info!("getting logs that should be applied to rollback state...");
        let logs = self
            .connection_pool
            .access_storage_blocking()
            .storage_logs_dal()
            .get_storage_logs_for_revert(last_l1_batch_to_keep);

        if rollback_tree {
            let storage_root_hash = self
                .connection_pool
                .access_storage_blocking()
                .blocks_dal()
                .get_merkle_state_root(last_l1_batch_to_keep)
                .expect("failed to fetch root hash for target block");

            // Convert H256 -> U256, note that tree keys are encoded using little endianness.
            let logs: Vec<_> = logs
                .iter()
                .map(|(key, value)| (U256::from_little_endian(&key.to_fixed_bytes()), *value))
                .collect();

            // Rolling back both full tree and lightweight tree
            let full_tree_path = self.config.db.path();
            if Path::new(full_tree_path).exists() {
                vlog::info!("Rolling back full tree...");
                Self::rollback_tree(
                    last_l1_batch_to_keep,
                    logs.clone(),
                    full_tree_path,
                    storage_root_hash,
                );
            } else {
                vlog::info!("Full tree not found; skipping");
            }

            let lightweight_tree_path = self.config.db.merkle_tree_fast_ssd_path();
            if Path::new(lightweight_tree_path).exists() {
                vlog::info!("Rolling back lightweight tree...");
                Self::rollback_tree(
                    last_l1_batch_to_keep,
                    logs,
                    lightweight_tree_path,
                    storage_root_hash,
                );
            } else {
                vlog::info!("Lightweight tree not found; skipping");
            }

            let new_lightweight_tree_path = &self.config.db.new_merkle_tree_ssd_path;
            if Path::new(new_lightweight_tree_path).exists() {
                vlog::info!("Rolling back new lightweight tree...");
                Self::rollback_new_tree(
                    last_l1_batch_to_keep,
                    new_lightweight_tree_path,
                    storage_root_hash,
                );
            } else {
                vlog::info!("New lightweight tree not found; skipping");
            }
        }

        if rollback_sk_cache {
            assert!(
                Path::new(self.config.db.state_keeper_db_path()).exists(),
                "Path with state keeper cache DB doesn't exist"
            );
            self.rollback_state_keeper_cache(last_l1_batch_to_keep, logs)
                .await;
        }
    }

    /// Reverts blocks in a Merkle tree.
    fn rollback_tree(
        last_l1_batch_to_keep: L1BatchNumber,
        logs: Vec<(U256, Option<H256>)>,
        path: impl AsRef<Path>,
        storage_root_hash: H256,
    ) {
        let db = RocksDB::new(Database::MerkleTree, path, true);
        let mut tree = ZkSyncTree::new(db);

        if tree.block_number() <= last_l1_batch_to_keep.0 {
            vlog::info!("Tree is behind the block to revert to; skipping");
            return;
        }
        tree.revert_logs(last_l1_batch_to_keep, logs);

        vlog::info!("checking match of the tree root hash and root hash from Postgres...");
        assert_eq!(tree.root_hash(), storage_root_hash);
        vlog::info!("saving tree changes to disk...");
        tree.save().expect("Unable to update tree state");
    }

    fn rollback_new_tree(
        last_l1_batch_to_keep: L1BatchNumber,
        path: impl AsRef<Path>,
        storage_root_hash: H256,
    ) {
        let db = RocksDB::new(Database::MerkleTree, path, true);
        let mut tree = NewTree::new_lightweight(db);

        if tree.block_number() <= last_l1_batch_to_keep.0 {
            vlog::info!("Tree is behind the block to revert to; skipping");
            return;
        }
        tree.revert_logs(last_l1_batch_to_keep);

        vlog::info!("checking match of the tree root hash and root hash from Postgres...");
        assert_eq!(tree.root_hash(), storage_root_hash);
        vlog::info!("saving tree changes to disk...");
        tree.save();
    }

    /// Reverts blocks in the state keeper cache.
    async fn rollback_state_keeper_cache(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
        logs: Vec<(H256, Option<H256>)>,
    ) {
        vlog::info!("opening DB with state keeper cache...");
        let db = RocksDB::new(
            Database::StateKeeper,
            self.config.db.state_keeper_db_path(),
            true,
        );
        let mut sk_cache = SecondaryStateStorage::new(db);

        if sk_cache.get_l1_batch_number() > last_l1_batch_to_keep + 1 {
            vlog::info!("getting contracts and factory deps that should be removed...");
            let mut storage = self.connection_pool.access_storage_blocking();
            let (_, last_miniblock_to_keep) = storage
                .blocks_dal()
                .get_miniblock_range_of_l1_batch(last_l1_batch_to_keep)
                .expect("L1 batch should contain at least one miniblock");
            let factory_deps = storage
                .storage_dal()
                .get_factory_deps_for_revert(last_miniblock_to_keep);

            vlog::info!("rolling back state keeper cache...");
            sk_cache.rollback(logs, factory_deps, last_l1_batch_to_keep);
        } else {
            vlog::info!("nothing to revert in state keeper cache");
        }
    }

    /// Reverts data in the Postgres database.
    fn rollback_postgres(&self, last_l1_batch_to_keep: L1BatchNumber) {
        vlog::info!("rolling back postgres data...");
        let mut storage = self.connection_pool.access_storage_blocking();
        let mut transaction = storage.start_transaction_blocking();

        let (_, last_miniblock_to_keep) = transaction
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(last_l1_batch_to_keep)
            .expect("L1 batch should contain at least one miniblock");

        vlog::info!("rolling back transactions state...");
        transaction
            .transactions_dal()
            .reset_transactions_state(last_miniblock_to_keep);
        vlog::info!("rolling back events...");
        transaction
            .events_dal()
            .rollback_events(last_miniblock_to_keep);
        vlog::info!("rolling back l2 to l1 logs...");
        transaction
            .events_dal()
            .rollback_l2_to_l1_logs(last_miniblock_to_keep);
        vlog::info!("rolling back created tokens...");
        transaction
            .tokens_dal()
            .rollback_tokens(last_miniblock_to_keep);
        vlog::info!("rolling back factory deps....");
        transaction
            .storage_dal()
            .rollback_factory_deps(last_miniblock_to_keep);
        vlog::info!("rolling back storage...");
        transaction
            .storage_logs_dal()
            .rollback_storage(last_miniblock_to_keep);
        vlog::info!("rolling back storage logs...");
        transaction
            .storage_logs_dal()
            .rollback_storage_logs(last_miniblock_to_keep);
        vlog::info!("rolling back l1 batches...");
        transaction
            .blocks_dal()
            .delete_l1_batches(last_l1_batch_to_keep);
        vlog::info!("rolling back miniblocks...");
        transaction
            .blocks_dal()
            .delete_miniblocks(last_miniblock_to_keep);

        transaction.commit_blocking();
    }

    /// Sends revert transaction to L1.
    pub async fn send_ethereum_revert_transaction(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
        priority_fee_per_gas: U256,
        nonce: u64,
    ) {
        let eth_gateway = PKSigningClient::from_config(&self.config);
        let contract = zksync_contract();
        let revert_blocks = contract
            .functions
            .get("revertBlocks")
            .expect("revertBlocks function not found")
            .last()
            .expect("revertBlocks function entry not found");
        let args = [Token::Uint(U256::from(last_l1_batch_to_keep.0))];
        let raw_tx = revert_blocks
            .encode_input(&args)
            .expect("Failed to encode transaction data.")
            .to_vec();
        let signed_tx = eth_gateway
            .sign_prepared_tx_for_addr(
                raw_tx,
                self.config.contracts.validator_timelock_addr,
                Options::with(|opt| {
                    opt.gas = Some(5_000_000.into());
                    opt.max_priority_fee_per_gas = Some(priority_fee_per_gas);
                    opt.nonce = Some(nonce.into());
                }),
                "block-reverter",
            )
            .await
            .expect("Failed to sign transaction");
        let tx_hash = eth_gateway
            .send_raw_tx(signed_tx.raw_tx)
            .await
            .expect("failed to send revert transaction to L1");

        loop {
            if let Some(status) = eth_gateway
                .get_tx_status(tx_hash, "block reverter")
                .await
                .expect("Failed to get tx status from eth node")
            {
                assert!(status.success);
                vlog::info!("revert transaction has completed");
                return;
            } else {
                vlog::info!("waiting for L1 transaction confirmation...");
                sleep(Duration::from_secs(5)).await;
            }
        }
    }

    async fn get_l1_batch_number_from_contract(&self, op: AggregatedActionType) -> L1BatchNumber {
        let function_name = match op {
            AggregatedActionType::CommitBlocks => "getTotalBlocksCommitted",
            AggregatedActionType::PublishProofBlocksOnchain => "getTotalBlocksVerified",
            AggregatedActionType::ExecuteBlocks => "getTotalBlocksExecuted",
        };
        let eth_gateway = PKSigningClient::from_config(&self.config);
        let block_number: U256 = eth_gateway
            .call_main_contract_function(function_name, (), None, Options::default(), None)
            .await
            .unwrap();
        L1BatchNumber(block_number.as_u32())
    }

    /// Returns suggested values for rollback.
    pub async fn suggested_values(&self) -> SuggestedRollbackValues {
        let eth_gateway = PKSigningClient::from_config(&self.config);
        let last_committed_l1_batch_number = self
            .get_l1_batch_number_from_contract(AggregatedActionType::CommitBlocks)
            .await;
        let last_verified_l1_batch_number = self
            .get_l1_batch_number_from_contract(AggregatedActionType::PublishProofBlocksOnchain)
            .await;
        let last_executed_l1_batch_number = self
            .get_l1_batch_number_from_contract(AggregatedActionType::ExecuteBlocks)
            .await;
        vlog::info!(
            "Last L1 batch numbers on contract: committed {}, verified {}, executed {}",
            last_committed_l1_batch_number,
            last_verified_l1_batch_number,
            last_executed_l1_batch_number
        );

        let nonce = eth_gateway
            .pending_nonce("reverter")
            .await
            .unwrap()
            .as_u64();
        let priority_fee = self
            .config
            .eth_sender
            .gas_adjuster
            .default_priority_fee_per_gas;

        SuggestedRollbackValues {
            last_executed_l1_batch_number,
            nonce,
            priority_fee,
        }
    }

    /// Clears failed L1 transactions
    pub async fn clear_failed_l1_transactions(&self) {
        vlog::info!("clearing failed L1 transactions...");
        self.connection_pool
            .access_storage_blocking()
            .eth_sender_dal()
            .clear_failed_transactions();
    }
}

#[derive(Debug, Serialize)]
pub struct SuggestedRollbackValues {
    pub last_executed_l1_batch_number: L1BatchNumber,
    pub nonce: u64,
    pub priority_fee: u64,
}
