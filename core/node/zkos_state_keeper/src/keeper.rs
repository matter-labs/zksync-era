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
use zk_os_forward_system::run::test_impl::{InMemoryPreimageSource, InMemoryTree};
use zk_os_system_hooks::addresses_constants::NOMINAL_TOKEN_BALANCE_STORAGE_ADDRESS;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_mempool::L2TxFilter;
use zksync_state::ReadStorageFactory;
use zksync_state_keeper::io::IoCursor;
use zksync_state_keeper::MempoolGuard;
use zksync_types::{Address, ERC20_TRANSFER_TOPIC, H256, L1BatchNumber, L2BlockNumber, StorageKey, StorageLog, Transaction};
use zksync_types::snapshots::SnapshotStorageLog;
use zksync_utils::time::{millis_since_epoch, seconds_since_epoch};
use crate::preimage_source::ZkSyncPreimageSource;
use crate::seal_logic::seal_in_db;
use crate::single_tx_source::SingleTxSource;
use zksync_zkos_vm_runner::zkos_conversions::{bytes32_to_h256, h256_to_bytes32};

const POLL_WAIT_DURATION: Duration = Duration::from_millis(50);

/// A stripped-down version of the state keeper that works with zk Os

/// Data layout changes:
/// * one miniblock = one batch (will get rid of one of the entities later on)
/// * in storage_logs we only consider `hashed_key` and `value` - `hashed_key` preimage (field `key`) is not used
/// * no initial_writes and protective_reads
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
    // storage_factory: Arc<dyn ReadStorageFactory>,
    mempool: MempoolGuard,
}

impl ZkosStateKeeper {
    pub fn new(
        stop_receiver: watch::Receiver<bool>,
        pool: ConnectionPool<Core>,
        // storage_factory: Arc<dyn ReadStorageFactory>,
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
            // storage_factory,
            preimage_source,
            mempool,
        }
    }
    pub async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!("Initializing ZkOs StateKeeper...");

        let mut connection = self.pool.connection_tagged("state_keeper").await?;
        // Note: snapshot recovery is not supported
        let cursor = IoCursor::new(&mut connection).await?;
        anyhow::ensure!(cursor.l1_batch.0 == cursor.next_l2_block.0, "For Zkos we expect batches to have just one l2 block each");

        // todo: maybe make part of the struct?
        let mut pending_block_number = cursor.next_l2_block;

        if pending_block_number.0 == 1 {
            tracing::info!("Setting initial balances for testing");
            for address in &[
                "0x27FBEc0B5D2A2B89f77e4D3648bBBBCF11784bdE",
                "0x2eF0972bd8AFc29d63b2412508ce5e20219b9A8c"
            ] {
                tracing::info!("setting balance for {}", address);

                let address = B160::from_str(address)?;
                let key = address_into_special_storage_key(&address);
                let balance = bytes32_to_h256(Bytes32::from_u256_be(U256::from(170000000)));
                let flat_key = bytes32_to_h256(derive_flat_storage_key(&NOMINAL_TOKEN_BALANCE_STORAGE_ADDRESS, &key));

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
                        &logs
                    )
                    .await?;
            }
        }

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
            let tx_source = SingleTxSource::new(tx);

            let context = BatchContext {
                eip1559_basefee: U256::from(1),
                ergs_price: U256::from(1),
                // todo: consider changing type
                block_number: pending_block_number.0 as u64,
                timestamp: seconds_since_epoch(),
            };

            let storage_commitment = StorageCommitment {
                root: self.tree.storage_tree.root().clone(),
                next_free_slot: self.tree.storage_tree.next_free_slot,
            };
            tracing::info!("Starting block {pending_block_number} with commitment root {:?} and next_free_slot {:?}",
                storage_commitment.root,
                storage_commitment.next_free_slot
            );

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
            // a little awkward but we need to insert both into cold_storage and storage_tree
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

        // todo: use proper filter
        let filter = L2TxFilter {
            fee_input: Default::default(),
            fee_per_gas: 0,
            gas_per_pubdata: 0,
        };

        let started_at = Instant::now();
        // or is it better to limit this?
        while !self.is_canceled() {
            // let get_latency = KEEPER_METRICS.get_tx_from_mempool.start();
            let maybe_tx = self.mempool.next_transaction(&filter);
            // get_latency.observe();

            if let Some((tx, _)) = maybe_tx {
                // Reject transactions with too big gas limit. They are also rejected on the API level, but
                // we need to secure ourselves in case some tx will somehow get into mempool.
                // if tx.gas_limit() > self.max_allowed_tx_gas_limit {
                //     tracing::warn!(
                //     "Found tx with too big gas limit in state keeper, hash: {:?}, gas_limit: {}",
                //     tx.hash(),
                //     tx.gas_limit()
                // );
                //     self.reject(&tx, UnexecutableReason::Halt(Halt::TooBigGasLimit))
                //         .await?;
                //     continue;
                // }
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