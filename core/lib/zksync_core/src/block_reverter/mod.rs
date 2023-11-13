use bitflags::bitflags;
use serde::Serialize;
use tokio::time::sleep;

use std::path::Path;
use std::time::Duration;

use zksync_config::{ContractsConfig, ETHSenderConfig};
use zksync_contracts::zksync_contract;
use zksync_dal::ConnectionPool;
use zksync_merkle_tree::domain::ZkSyncTree;
use zksync_state::RocksdbStorage;
use zksync_storage::RocksDB;
use zksync_types::aggregated_operations::AggregatedActionType;
use zksync_types::ethabi::Token;
use zksync_types::web3::{
    contract::{Contract, Options},
    transports::Http,
    types::{BlockId, BlockNumber},
    Web3,
};
use zksync_types::{L1BatchNumber, PackedEthSignature, H160, H256, U256};

use zksync_eth_signer::{EthereumSigner, PrivateKeySigner, TransactionParameters};

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

#[derive(Debug)]
pub struct BlockReverterEthConfig {
    eth_client_url: String,
    reverter_private_key: H256,
    reverter_address: H160,
    diamond_proxy_addr: H160,
    validator_timelock_addr: H160,
    default_priority_fee_per_gas: u64,
}

impl BlockReverterEthConfig {
    pub fn new(eth_config: ETHSenderConfig, contract: ContractsConfig, web3_url: String) -> Self {
        let pk = eth_config
            .sender
            .private_key()
            .expect("Private key is required for block reversion");
        let operator_address = PackedEthSignature::address_from_private_key(&pk)
            .expect("Failed to get address from private key");

        Self {
            eth_client_url: web3_url,
            reverter_private_key: pk,
            reverter_address: operator_address,
            diamond_proxy_addr: contract.diamond_proxy_addr,
            validator_timelock_addr: contract.validator_timelock_addr,
            default_priority_fee_per_gas: eth_config.gas_adjuster.default_priority_fee_per_gas,
        }
    }
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
    state_keeper_cache_path: String,
    merkle_tree_path: String,
    eth_config: Option<BlockReverterEthConfig>,
    connection_pool: ConnectionPool,
    executed_batches_revert_mode: L1ExecutedBatchesRevert,
}

impl BlockReverter {
    pub fn new(
        state_keeper_cache_path: String,
        merkle_tree_path: String,
        eth_config: Option<BlockReverterEthConfig>,
        connection_pool: ConnectionPool,
        executed_batches_revert_mode: L1ExecutedBatchesRevert,
    ) -> Self {
        Self {
            state_keeper_cache_path,
            merkle_tree_path,
            eth_config,
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
            let mut storage = self.connection_pool.access_storage().await.unwrap();
            let last_executed_l1_batch = storage
                .blocks_dal()
                .get_number_of_last_l1_batch_executed_on_eth()
                .await
                .unwrap()
                .expect("failed to get last executed L1 batch");
            assert!(
                last_l1_batch_to_keep >= last_executed_l1_batch,
                "Attempt to revert already executed L1 batches"
            );
        }

        // Tree needs to be reverted first to keep state recoverable
        self.rollback_rocks_dbs(last_l1_batch_to_keep, rollback_tree, rollback_sk_cache)
            .await;
        if rollback_postgres {
            self.rollback_postgres(last_l1_batch_to_keep).await;
        }
    }

    async fn rollback_rocks_dbs(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
        rollback_tree: bool,
        rollback_sk_cache: bool,
    ) {
        if rollback_tree {
            let storage_root_hash = self
                .connection_pool
                .access_storage()
                .await
                .unwrap()
                .blocks_dal()
                .get_l1_batch_state_root(last_l1_batch_to_keep)
                .await
                .unwrap()
                .expect("failed to fetch root hash for target L1 batch");

            // Rolling back Merkle tree
            let merkle_tree_path = Path::new(&self.merkle_tree_path);
            if merkle_tree_path.exists() {
                tracing::info!("Rolling back Merkle tree...");
                Self::rollback_new_tree(last_l1_batch_to_keep, merkle_tree_path, storage_root_hash);
            } else {
                tracing::info!("Merkle tree not found; skipping");
            }
        }

        if rollback_sk_cache {
            assert!(
                Path::new(&self.state_keeper_cache_path).exists(),
                "Path with state keeper cache DB doesn't exist"
            );
            self.rollback_state_keeper_cache(last_l1_batch_to_keep)
                .await;
        }
    }

    fn rollback_new_tree(
        last_l1_batch_to_keep: L1BatchNumber,
        path: &Path,
        storage_root_hash: H256,
    ) {
        let db = RocksDB::new(path);
        let mut tree = ZkSyncTree::new_lightweight(db);

        if tree.next_l1_batch_number() <= last_l1_batch_to_keep {
            tracing::info!("Tree is behind the L1 batch to revert to; skipping");
            return;
        }
        tree.revert_logs(last_l1_batch_to_keep);

        tracing::info!("checking match of the tree root hash and root hash from Postgres...");
        assert_eq!(tree.root_hash(), storage_root_hash);
        tracing::info!("saving tree changes to disk...");
        tree.save();
    }

    /// Reverts blocks in the state keeper cache.
    async fn rollback_state_keeper_cache(&self, last_l1_batch_to_keep: L1BatchNumber) {
        tracing::info!("opening DB with state keeper cache...");
        let mut sk_cache = RocksdbStorage::new(self.state_keeper_cache_path.as_ref());

        if sk_cache.l1_batch_number() > last_l1_batch_to_keep + 1 {
            let mut storage = self.connection_pool.access_storage().await.unwrap();
            tracing::info!("rolling back state keeper cache...");
            sk_cache.rollback(&mut storage, last_l1_batch_to_keep).await;
        } else {
            tracing::info!("nothing to revert in state keeper cache");
        }
    }

    /// Reverts data in the Postgres database.
    async fn rollback_postgres(&self, last_l1_batch_to_keep: L1BatchNumber) {
        tracing::info!("rolling back postgres data...");
        let mut storage = self.connection_pool.access_storage().await.unwrap();
        let mut transaction = storage.start_transaction().await.unwrap();

        let (_, last_miniblock_to_keep) = transaction
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(last_l1_batch_to_keep)
            .await
            .unwrap()
            .expect("L1 batch should contain at least one miniblock");

        tracing::info!("rolling back transactions state...");
        transaction
            .transactions_dal()
            .reset_transactions_state(last_miniblock_to_keep)
            .await;
        tracing::info!("rolling back events...");
        transaction
            .events_dal()
            .rollback_events(last_miniblock_to_keep)
            .await;
        tracing::info!("rolling back l2 to l1 logs...");
        transaction
            .events_dal()
            .rollback_l2_to_l1_logs(last_miniblock_to_keep)
            .await;
        tracing::info!("rolling back created tokens...");
        transaction
            .tokens_dal()
            .rollback_tokens(last_miniblock_to_keep)
            .await;
        tracing::info!("rolling back factory deps....");
        transaction
            .storage_dal()
            .rollback_factory_deps(last_miniblock_to_keep)
            .await;
        tracing::info!("rolling back storage...");
        transaction
            .storage_logs_dal()
            .rollback_storage(last_miniblock_to_keep)
            .await;
        tracing::info!("rolling back storage logs...");
        transaction
            .storage_logs_dal()
            .rollback_storage_logs(last_miniblock_to_keep)
            .await;
        tracing::info!("rolling back l1 batches...");
        transaction
            .blocks_dal()
            .delete_l1_batches(last_l1_batch_to_keep)
            .await
            .unwrap();
        tracing::info!("rolling back miniblocks...");
        transaction
            .blocks_dal()
            .delete_miniblocks(last_miniblock_to_keep)
            .await
            .unwrap();

        transaction.commit().await.unwrap();
    }

    /// Sends revert transaction to L1.
    pub async fn send_ethereum_revert_transaction(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
        priority_fee_per_gas: U256,
        nonce: u64,
    ) {
        let eth_config = self
            .eth_config
            .as_ref()
            .expect("eth_config is not provided");

        let web3 = Web3::new(Http::new(&eth_config.eth_client_url).unwrap());
        let contract = zksync_contract();
        let signer = PrivateKeySigner::new(eth_config.reverter_private_key);
        let chain_id = web3.eth().chain_id().await.unwrap().as_u64();

        let revert_function = contract
            .function("revertBlocks")
            .or_else(|_| contract.function("revertBatches"))
            .expect(
                "Either `revertBlocks` or `revertBatches` function must be present in contract",
            );
        let data = revert_function
            .encode_input(&[Token::Uint(last_l1_batch_to_keep.0.into())])
            .unwrap();

        let base_fee = web3
            .eth()
            .block(BlockId::Number(BlockNumber::Pending))
            .await
            .unwrap()
            .unwrap()
            .base_fee_per_gas
            .unwrap();

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

        let signed_tx = signer.sign_transaction(tx).await.unwrap();
        let hash = web3
            .eth()
            .send_raw_transaction(signed_tx.into())
            .await
            .unwrap();

        loop {
            if let Some(receipt) = web3.eth().transaction_receipt(hash).await.unwrap() {
                assert_eq!(receipt.status, Some(1.into()), "revert transaction failed");
                tracing::info!("revert transaction has completed");
                return;
            } else {
                tracing::info!("waiting for L1 transaction confirmation...");
                sleep(Duration::from_secs(5)).await;
            }
        }
    }

    async fn get_l1_batch_number_from_contract(&self, op: AggregatedActionType) -> L1BatchNumber {
        let function_name = match op {
            AggregatedActionType::Commit => "getTotalBatchesCommitted",
            AggregatedActionType::PublishProofOnchain => "getTotalBatchesVerified",
            AggregatedActionType::Execute => "getTotalBatchesExecuted",
        };
        let eth_config = self
            .eth_config
            .as_ref()
            .expect("eth_config is not provided");

        let web3 = Web3::new(Http::new(&eth_config.eth_client_url).unwrap());
        let contract = {
            let abi = zksync_contract();
            let contract_address = eth_config.diamond_proxy_addr;
            Contract::new(web3.eth(), contract_address, abi)
        };

        let block_number: U256 = contract
            .query(function_name, (), None, Options::default(), None)
            .await
            .unwrap();

        L1BatchNumber(block_number.as_u32())
    }

    /// Returns suggested values for rollback.
    pub async fn suggested_values(&self) -> SuggestedRollbackValues {
        let last_committed_l1_batch_number = self
            .get_l1_batch_number_from_contract(AggregatedActionType::Commit)
            .await;
        let last_verified_l1_batch_number = self
            .get_l1_batch_number_from_contract(AggregatedActionType::PublishProofOnchain)
            .await;
        let last_executed_l1_batch_number = self
            .get_l1_batch_number_from_contract(AggregatedActionType::Execute)
            .await;
        tracing::info!(
            "Last L1 batch numbers on contract: committed {last_committed_l1_batch_number}, \
             verified {last_verified_l1_batch_number}, executed {last_executed_l1_batch_number}"
        );

        let eth_config = self
            .eth_config
            .as_ref()
            .expect("eth_config is not provided");

        let priority_fee = eth_config.default_priority_fee_per_gas;

        let web3 = Web3::new(Http::new(&eth_config.eth_client_url).unwrap());
        let nonce = web3
            .eth()
            .transaction_count(eth_config.reverter_address, Some(BlockNumber::Pending))
            .await
            .unwrap()
            .as_u64();

        SuggestedRollbackValues {
            last_executed_l1_batch_number,
            nonce,
            priority_fee,
        }
    }

    /// Clears failed L1 transactions
    pub async fn clear_failed_l1_transactions(&self) {
        tracing::info!("clearing failed L1 transactions...");
        self.connection_pool
            .access_storage()
            .await
            .unwrap()
            .eth_sender_dal()
            .clear_failed_transactions()
            .await
            .unwrap();
    }

    pub fn change_rollback_executed_l1_batches_allowance(
        &mut self,
        revert_executed_batches: L1ExecutedBatchesRevert,
    ) {
        self.executed_batches_revert_mode = revert_executed_batches
    }
}

#[derive(Debug, Serialize)]
pub struct SuggestedRollbackValues {
    pub last_executed_l1_batch_number: L1BatchNumber,
    pub nonce: u64,
    pub priority_fee: u64,
}
