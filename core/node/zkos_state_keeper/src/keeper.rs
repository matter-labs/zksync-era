use std::alloc::Global;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use anyhow::Context;
use ruint::aliases::U256;
use ruint::aliases::B160;
use zk_os_forward_system::run::{BatchContext, BatchOutput, PreimageSource, run_batch, StorageCommitment};
use tokio::sync::watch;
use tokio::task::spawn_blocking;
use tokio::time::Instant;
use tracing::info_span;
use zk_ee::common_structs::derive_flat_storage_key;
use zk_ee::system::ExecutionEnvironmentType;
use zk_ee::system::system_io_oracle::PreimageType;
use zk_ee::utils::Bytes32;
use zk_os_basic_system::basic_io_implementer::address_into_special_storage_key;
use zk_os_basic_system::basic_system::simple_growable_storage::TestingTree;
use zk_os_forward_system::run::test_impl::{InMemoryPreimageSource, InMemoryTree, TxListSource};
use zk_os_system_hooks::addresses_constants::NOMINAL_TOKEN_BALANCE_STORAGE_ADDRESS;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_mempool::L2TxFilter;
use zksync_state::ReadStorageFactory;
use zksync_state_keeper::io::IoCursor;
use zksync_state_keeper::MempoolGuard;
use zksync_types::{Address, ERC20_TRANSFER_TOPIC, H256, L1BatchNumber, L2BlockNumber, StorageKey, StorageLog, Transaction};
use zksync_types::snapshots::SnapshotStorageLog;
use crate::seal_logic::seal_in_db;
use zksync_zkos_vm_runner::zkos_conversions::{bytes32_to_h256, h256_to_bytes32, tx_abi_encode};
use crate::millis_since_epoch;

const POLL_WAIT_DURATION: Duration = Duration::from_millis(50);

/// A stripped-down version of the state keeper that supports zk Os

/// Data layout changes:
/// * one miniblock = one batch = one transaction
/// * in storage_logs we only consider `hashed_key` and `value` - `hashed_key` preimage (field `key`) is not used
/// * no initial_writes and protective_reads
/// * no transaction replacement
///
///
/// transaction results are perstisted in the DB, but the tree is recomputed from scratch on each startup
/// no RocksDB cache is used
///
///
pub struct ZkosStateKeeper {
    stop_receiver: watch::Receiver<bool>,

    tree: InMemoryTree,
    preimage_source: InMemoryPreimageSource,

    pool: ConnectionPool<Core>,
    mempool: MempoolGuard,
}

impl ZkosStateKeeper {
    pub fn new(
        stop_receiver: watch::Receiver<bool>,
        pool: ConnectionPool<Core>,
        mempool: MempoolGuard,
    ) -> Self {
        let tree = InMemoryTree {
            storage_tree: TestingTree::new_in(Global),
            cold_storage: HashMap::new(),
        };
        let preimage_source = InMemoryPreimageSource {
            inner: Default::default(),
        };
        Self {
            stop_receiver,
            pool,
            tree,
            preimage_source,
            mempool,
        }
    }
    pub async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!("Initializing ZkOs StateKeeper...");

        let mut connection = self.pool.connection_tagged("state_keeper").await?;
        let cursor = IoCursor::new(&mut connection).await?;
        anyhow::ensure!(cursor.l1_batch.0 == cursor.next_l2_block.0, "For Zkos we expect batches to have just one l2 block each");

        let mut pending_block_number = cursor.next_l2_block;

        Self::fund_dev_wallets_if_needed(&mut connection, &mut pending_block_number).await;

        self.initialize_in_memory_storages().await?;

        while !self.is_canceled() {
            tracing::info!("Waiting for the next transaction");

            let Some(tx) = self
                .wait_for_next_tx()
                .await else {
                return Ok(())
            };
            tracing::info!("Transaction found: {:?}", tx);
            let tx_hash = tx.hash();
            let encoded = tx_abi_encode(tx);
            let tx_source = TxListSource {
                transactions: vec![encoded].into(),
            };

            let context = BatchContext {
                //todo: gas
                eip1559_basefee: U256::from(1),
                ergs_price: U256::from(1),
                gas_per_pubdata: Default::default(),
                block_number: pending_block_number.0 as u64,
                timestamp: (millis_since_epoch() / 1000) as u64,
            };

            let storage_commitment = StorageCommitment {
                root: self.tree.storage_tree.root().clone(),
                next_free_slot: self.tree.storage_tree.next_free_slot,
            };
            tracing::info!("Starting block {pending_block_number} with commitment root {:?} and next_free_slot {:?}",
                storage_commitment.root,
                storage_commitment.next_free_slot
            );

            //todo: at least use refcell instead of cloning
            tracing::info!("Cloning in-memory storages for the batch");
            let tree = self.tree.clone();
            let preimage_source = self.preimage_source.clone();
            tracing::info!("Cloning done, running batch");

            let result =
                spawn_blocking(move ||
                run_batch(
                    context,
                    storage_commitment,
                    tree,
                    preimage_source,
                    tx_source,
                ))
                    .await
                    .expect("Task panicked");

            match result {
                Ok(result) => {
                    tracing::info!("Batch executed successfully: {:?}", result);

                    for storage_write in result.storage_writes.iter() {
                        self.tree.cold_storage.insert(
                            storage_write.key,
                            storage_write.value,
                        );
                        self.tree.storage_tree.insert(
                            &storage_write.key,
                            &storage_write.value,
                        );
                    }

                    for (hash, preimage) in result.published_preimages.iter() {
                        self.preimage_source.inner.insert(
                            (
                                PreimageType::Bytecode(ExecutionEnvironmentType::EVM),
                                *hash,
                            ),
                            preimage.clone(),
                        );
                    }

                    let conn = self
                        .pool
                        .connection_tagged("zkos_state_keeper_seal_block")
                        .await?;

                    seal_in_db(conn, context, &result, tx_hash, H256::zero()).await?;
                    pending_block_number.0 += 1;
                }
                Err(err) => {
                    tracing::error!("Error running batch: {:?}", err);
                }
            }
        }
        Ok(())
    }

    // Funds dev wallets with some ETH for testing
    // only funds wallets that were not funded before
    // wallets can be added to this list without regenesis
    async fn fund_dev_wallets_if_needed(connection: &mut Connection<'_, Core>, pending_block_number: &mut L2BlockNumber) {
        for address in &[
            "0x27FBEc0B5D2A2B89f77e4D3648bBBBCF11784bdE",
            "0x2eF0972bd8AFc29d63b2412508ce5e20219b9A8c",
            "0xBC989fDe9e54cAd2aB4392Af6dF60f04873A033A"
        ] {
            let address = B160::from_str(address).unwrap();
            let key = address_into_special_storage_key(&address);
            let balance = bytes32_to_h256(Bytes32::from_u256_be(U256::from_str("1700000000000000000").unwrap()));;
            let flat_key = bytes32_to_h256(derive_flat_storage_key(&NOMINAL_TOKEN_BALANCE_STORAGE_ADDRESS, &key));

            let r = connection
                .storage_logs_dal()
                .get_storage_values(
                    &[flat_key],
                    *pending_block_number,
                ).await.expect("Failed to get storage values for initial balances");
            if r.get(&flat_key).cloned().unwrap_or_default().is_some() {
                tracing::info!("Wallet {:?} already funded", address);
                continue;
            }
            tracing::info!("Funding wallet {:?}", address);


            let logs = [SnapshotStorageLog {
                key: flat_key,
                value: balance,
                l1_batch_number_of_initial_write: Default::default(),
                enumeration_index: 0,
            }];

            connection
                .storage_logs_dal()
                .insert_storage_logs_from_snapshot(
                    L2BlockNumber(0),
                    &logs,
                )
                .await
                .expect("Failed to insert storage logs for initial balances");
        }
    }

    async fn initialize_in_memory_storages(&mut self) -> anyhow::Result<()> {
        let mut conn = self
            .pool
            .connection_tagged("zkos_state_keeper")
            .await?;

        let all_storage_logs = conn
            .storage_logs_dal()
            .dump_all_storage_logs_for_tests()
            .await;

        let preimages = conn
            .factory_deps_dal()
            .dump_all_factory_deps_for_tests()
            .await;

        tracing::info!("Loaded from DB: {:?} storage logs and {:?} preimages", all_storage_logs.len(), preimages.len());
        tracing::info!("Recovering tree from storage logs...");

        for storage_logs in all_storage_logs {
            // todo: awkwardly we need to insert both into cold_storage and storage_tree
            self.tree.cold_storage.insert(
                h256_to_bytes32(storage_logs.hashed_key),
                h256_to_bytes32(storage_logs.value),
            );
            self.tree.storage_tree.insert(
                &h256_to_bytes32(storage_logs.hashed_key),
                &h256_to_bytes32(storage_logs.value),
            );
        }
        tracing::info!("Tree recovery complete");

        tracing::info!("Recovering preimages...");

        for (hash, value) in preimages {
            self.preimage_source.inner.insert(
                (
                    PreimageType::Bytecode(ExecutionEnvironmentType::EVM),
                    h256_to_bytes32(hash),
                ),
                value,
            );
        }

        tracing::info!("Preimage recovery complete");
        Ok(())
    }

    async fn wait_for_next_tx(&mut self) -> Option<Transaction> {

        // todo: gas - use proper filter
        let filter = L2TxFilter {
            fee_input: Default::default(),
            fee_per_gas: 0,
            gas_per_pubdata: 0,
        };

        let started_at = Instant::now();
        while !self.is_canceled() {
            let maybe_tx = self.mempool.next_transaction(&filter);
            if let Some((tx, _)) = maybe_tx {
                //todo: reject transactions with too big gas limit. They are also rejected on the API level, but
                // we need to secure ourselves in case some tx will somehow get into mempool.
                return Some(tx);
            }
            tokio::time::sleep(POLL_WAIT_DURATION).await;
        }
        None
    }


    fn is_canceled(&self) -> bool {
        *self.stop_receiver.borrow()
    }
}