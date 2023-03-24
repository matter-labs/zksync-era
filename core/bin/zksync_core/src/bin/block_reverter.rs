use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use structopt::StructOpt;

use zksync_config::ZkSyncConfig;
use zksync_contracts::zksync_contract;
use zksync_dal::ConnectionPool;
use zksync_eth_client::clients::http_client::{EthInterface, EthereumClient};
use zksync_merkle_tree::ZkSyncTree;
use zksync_state::secondary_storage::SecondaryStateStorage;
use zksync_storage::db::Database;
use zksync_storage::RocksDB;
use zksync_types::aggregated_operations::AggregatedActionType;
use zksync_types::ethabi::Token;
use zksync_types::web3::contract::Options;
use zksync_types::{L1BatchNumber, H256, U256};

struct BlockReverter {
    config: ZkSyncConfig,
    connection_pool: ConnectionPool,
}

impl BlockReverter {
    /// rollback db(postgres + rocksdb) to previous state
    async fn rollback_db(
        &mut self,
        last_l1_batch_to_keep: L1BatchNumber,
        rollback_postgres: bool,
        rollback_tree: bool,
        rollback_sk_cache: bool,
    ) {
        let last_executed_l1_batch = self
            .get_l1_batch_number_from_contract(AggregatedActionType::ExecuteBlocks)
            .await;
        assert!(
            last_l1_batch_to_keep >= last_executed_l1_batch,
            "Attempt to revert already executed blocks"
        );

        if !rollback_tree && rollback_postgres {
            println!("You want to rollback Postgres DB without rolling back tree.");
            println!("If tree is not yet rolled back to this block then the only way to make it synced with Postgres will be to completely rebuild it.");
            println!("Are you sure? Print y/n");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            if input.trim() != "y" {
                std::process::exit(0);
            }
        }

        // tree needs to be reverted first to keep state recoverable
        self.rollback_rocks_dbs(last_l1_batch_to_keep, rollback_tree, rollback_sk_cache)
            .await;

        if rollback_postgres {
            self.rollback_postgres(last_l1_batch_to_keep).await;
        }
    }

    async fn rollback_rocks_dbs(
        &mut self,
        last_l1_batch_to_keep: L1BatchNumber,
        rollback_tree: bool,
        rollback_sk_cache: bool,
    ) {
        println!("getting logs that should be applied to rollback state...");
        let logs = self
            .connection_pool
            .access_storage()
            .await
            .storage_logs_dedup_dal()
            .get_storage_logs_for_revert(last_l1_batch_to_keep);

        if rollback_tree {
            // Rolling back both full tree and lightweight tree
            if Path::new(self.config.db.path()).exists() {
                println!("Rolling back full tree...");
                self.rollback_tree(
                    last_l1_batch_to_keep,
                    logs.clone(),
                    self.config.db.path.clone(),
                )
                .await;
            } else {
                println!("Full Tree not found; skipping");
            }

            if Path::new(self.config.db.merkle_tree_fast_ssd_path()).exists() {
                println!("Rolling back lightweight tree...");
                self.rollback_tree(
                    last_l1_batch_to_keep,
                    logs.clone(),
                    self.config.db.merkle_tree_fast_ssd_path.clone(),
                )
                .await;
            } else {
                println!("Lightweight Tree not found; skipping");
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

    /// reverts blocks in merkle tree
    async fn rollback_tree(
        &mut self,
        last_l1_batch_to_keep: L1BatchNumber,
        logs: Vec<(H256, Option<H256>)>,
        path: impl AsRef<Path>,
    ) {
        let db = RocksDB::new(Database::MerkleTree, path, true);
        let mut tree = ZkSyncTree::new(db);

        if tree.block_number() <= last_l1_batch_to_keep.0 {
            println!("Tree is behind the block to revert to; skipping");
            return;
        }

        // Convert H256 -> U256, note that tree keys are encoded using little endianness.
        let logs: Vec<_> = logs
            .into_iter()
            .map(|(key, value)| (U256::from_little_endian(&key.to_fixed_bytes()), value))
            .collect();
        tree.revert_logs(last_l1_batch_to_keep, logs);

        println!("checking match of the tree root hash and root hash from Postgres...");
        let storage_root_hash = self
            .connection_pool
            .access_storage()
            .await
            .blocks_dal()
            .get_merkle_state_root(last_l1_batch_to_keep)
            .expect("failed to fetch root hash for target block");
        let tree_root_hash = tree.root_hash();
        assert_eq!(&tree_root_hash, storage_root_hash.as_bytes());

        println!("saving tree changes to disk...");
        tree.save().expect("Unable to update tree state");
    }

    /// reverts blocks in state keeper cache
    async fn rollback_state_keeper_cache(
        &mut self,
        last_l1_batch_to_keep: L1BatchNumber,
        logs: Vec<(H256, Option<H256>)>,
    ) {
        println!("opening DB with state keeper cache...");
        let db = RocksDB::new(
            Database::StateKeeper,
            self.config.db.state_keeper_db_path(),
            true,
        );
        let mut storage = SecondaryStateStorage::new(db);

        if storage.get_l1_batch_number() > last_l1_batch_to_keep + 1 {
            println!("getting contracts and factory deps that should be removed...");
            let (_, last_miniblock_to_keep) = self
                .connection_pool
                .access_storage()
                .await
                .blocks_dal()
                .get_miniblock_range_of_l1_batch(last_l1_batch_to_keep)
                .expect("L1 batch should contain at least one miniblock");
            let contracts = self
                .connection_pool
                .access_storage()
                .await
                .storage_dal()
                .get_contracts_for_revert(last_miniblock_to_keep);
            let factory_deps = self
                .connection_pool
                .access_storage()
                .await
                .storage_dal()
                .get_factory_deps_for_revert(last_miniblock_to_keep);

            println!("rolling back state keeper cache...");
            storage.rollback(logs, contracts, factory_deps, last_l1_batch_to_keep);
        } else {
            println!("nothing to revert in state keeper cache");
        }
    }

    /// reverts data in postgres database
    async fn rollback_postgres(&mut self, last_l1_batch_to_keep: L1BatchNumber) {
        let (_, last_miniblock_to_keep) = self
            .connection_pool
            .access_storage()
            .await
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(last_l1_batch_to_keep)
            .expect("L1 batch should contain at least one miniblock");

        println!("rolling back postgres data...");
        let mut storage = self.connection_pool.access_storage().await;
        let mut transaction = storage.start_transaction().await;

        println!("rolling back transactions state...");
        transaction
            .transactions_dal()
            .reset_transactions_state(last_miniblock_to_keep);
        println!("rolling back events...");
        transaction
            .events_dal()
            .rollback_events(last_miniblock_to_keep);
        println!("rolling back l2 to l1 logs...");
        transaction
            .events_dal()
            .rollback_l2_to_l1_logs(last_miniblock_to_keep);
        println!("rolling back created tokens...");
        transaction
            .tokens_dal()
            .rollback_tokens(last_miniblock_to_keep);
        println!("rolling back factory deps....");
        transaction
            .storage_dal()
            .rollback_factory_deps(last_miniblock_to_keep);
        println!("rolling back storage...");
        transaction
            .storage_logs_dal()
            .rollback_storage(last_miniblock_to_keep);
        println!("rolling back storage logs...");
        transaction
            .storage_logs_dal()
            .rollback_storage_logs(last_miniblock_to_keep);
        println!("rolling back dedup storage logs...");
        transaction
            .storage_logs_dedup_dal()
            .rollback_storage_logs(last_l1_batch_to_keep);
        println!("rolling back l1 batches...");
        transaction
            .blocks_dal()
            .delete_l1_batches(last_l1_batch_to_keep);
        println!("rolling back miniblocks...");
        transaction
            .blocks_dal()
            .delete_miniblocks(last_miniblock_to_keep);

        transaction.commit().await;
    }

    /// sends revert transaction to L1
    async fn send_ethereum_revert_transaction(
        &mut self,
        last_l1_batch_to_keep: L1BatchNumber,
        priority_fee_per_gas: U256,
        nonce: u64,
    ) {
        let eth_gateway = EthereumClient::from_config(&self.config);
        let revert_blocks = zksync_contract()
            .functions
            .get("revertBlocks")
            .cloned()
            .expect("revertBlocks function not found")
            .pop()
            .expect("revertBlocks function entry not found");
        let args = vec![Token::Uint(U256::from(last_l1_batch_to_keep.0))];
        let raw_tx = revert_blocks
            .encode_input(&args)
            .expect("Failed to encode transaction data.")
            .to_vec();
        let signed_tx = eth_gateway
            .sign_prepared_tx_for_addr(
                raw_tx.clone(),
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
            match eth_gateway
                .get_tx_status(tx_hash, "block reverter")
                .await
                .expect("Failed to get tx status from eth node")
            {
                Some(status) => {
                    assert!(status.success);
                    println!("revert transaction has completed");
                    return;
                }
                None => {
                    println!("waiting for L1 transaction confirmation...");
                    sleep(Duration::from_secs(5));
                }
            }
        }
    }

    async fn get_l1_batch_number_from_contract(&self, op: AggregatedActionType) -> L1BatchNumber {
        let function_name = match op {
            AggregatedActionType::CommitBlocks => "getTotalBlocksCommitted",
            AggregatedActionType::PublishProofBlocksOnchain => "getTotalBlocksVerified",
            AggregatedActionType::ExecuteBlocks => "getTotalBlocksExecuted",
        };
        let eth_gateway = EthereumClient::from_config(&self.config);
        let block_number: U256 = eth_gateway
            .call_main_contract_function(function_name, (), None, Options::default(), None)
            .await
            .unwrap();
        L1BatchNumber(block_number.as_u32())
    }

    /// displays suggested values for rollback
    async fn print_suggested_values(&mut self) {
        let eth_gateway = EthereumClient::from_config(&self.config);
        let last_committed_l1_batch_number = self
            .get_l1_batch_number_from_contract(AggregatedActionType::CommitBlocks)
            .await;
        let last_verified_l1_batch_number = self
            .get_l1_batch_number_from_contract(AggregatedActionType::PublishProofBlocksOnchain)
            .await;
        let last_executed_l1_batch_number = self
            .get_l1_batch_number_from_contract(AggregatedActionType::ExecuteBlocks)
            .await;
        println!(
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
        println!("Suggested values for rollback:");
        println!("      l1 batch number: {}", last_executed_l1_batch_number.0);
        println!("      nonce: {}", nonce);
        println!(
            "      priority fee: {:?}",
            self.config
                .eth_sender
                .gas_adjuster
                .default_priority_fee_per_gas
        );
    }

    /// Clears failed L1 transactions
    async fn clear_failed_l1_transactions(&mut self) {
        println!("clearing failed L1 transactions...");
        self.connection_pool
            .access_storage()
            .await
            .eth_sender_dal()
            .clear_failed_transactions();
    }
}

#[derive(StructOpt, Debug)]
#[structopt(name = "block revert utility")]
enum Opt {
    #[structopt(
        name = "print-suggested-values",
        about = "Displays suggested values to use"
    )]
    Display,

    #[structopt(
        name = "send-eth-transaction",
        about = "Sends revert transaction to L1"
    )]
    SendEthTransaction {
        /// L1 batch number used to rollback to
        #[structopt(long)]
        l1_batch_number: u32,

        /// Priority fee used for rollback ethereum transaction
        // We operate only by priority fee because we want to use base fee from ethereum
        // and send transaction as soon as possible without any resend logic
        #[structopt(long)]
        priority_fee_per_gas: Option<u64>,

        /// Nonce used for rollback ethereum transaction
        #[structopt(long)]
        nonce: u64,
    },

    #[structopt(
        name = "rollback-db",
        about = "Reverts internal database state to previous block"
    )]
    RollbackDB {
        /// L1 batch number used to rollback to
        #[structopt(long)]
        l1_batch_number: u32,
        /// Flag that specifies if Postgres DB should be rolled back.
        #[structopt(long)]
        rollback_postgres: bool,
        /// Flag that specifies if RocksDB with tree should be rolled back.
        #[structopt(long)]
        rollback_tree: bool,
        /// Flag that specifies if RocksDB with state keeper cache should be rolled back.
        #[structopt(long)]
        rollback_sk_cache: bool,
    },

    #[structopt(
        name = "clear-failed-transactions",
        about = "Clears failed L1 transactions"
    )]
    ClearFailedL1Transactions,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _sentry_guard = vlog::init();
    let config = ZkSyncConfig::from_env();
    let connection_pool = ConnectionPool::new(None, true);
    let mut block_reverter = BlockReverter {
        config: config.clone(),
        connection_pool: connection_pool.clone(),
    };

    match Opt::from_args() {
        Opt::Display => block_reverter.print_suggested_values().await,
        Opt::SendEthTransaction {
            l1_batch_number,
            priority_fee_per_gas,
            nonce,
        } => {
            let priority_fee_per_gas = priority_fee_per_gas.map(U256::from).unwrap_or_else(|| {
                U256::from(
                    block_reverter
                        .config
                        .eth_sender
                        .gas_adjuster
                        .default_priority_fee_per_gas,
                )
            });
            block_reverter
                .send_ethereum_revert_transaction(
                    L1BatchNumber(l1_batch_number),
                    priority_fee_per_gas,
                    nonce,
                )
                .await
        }
        Opt::RollbackDB {
            l1_batch_number,
            rollback_postgres,
            rollback_tree,
            rollback_sk_cache,
        } => {
            block_reverter
                .rollback_db(
                    L1BatchNumber(l1_batch_number),
                    rollback_postgres,
                    rollback_tree,
                    rollback_sk_cache,
                )
                .await
        }
        Opt::ClearFailedL1Transactions => block_reverter.clear_failed_l1_transactions().await,
    }
    Ok(())
}
