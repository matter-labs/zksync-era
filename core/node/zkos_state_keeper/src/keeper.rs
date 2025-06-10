use std::{alloc::Global, collections::HashMap, str::FromStr, sync::Arc, time::Duration};

use anyhow::Context;
use ruint::aliases::{B160, U256};
use serde::Serialize;
use tokio::{
    sync::{
        mpsc::{error::TryRecvError, Receiver, Sender},
        watch,
    },
    task::{spawn_blocking, JoinHandle},
    time::Instant,
};
use tracing::info_span;
use zk_ee::{
    common_structs::derive_flat_storage_key, utils::Bytes32,
};
use zk_os_basic_system::system_implementation::flat_storage_model::TestingTree;
use zk_os_forward_system::run::{
    result_keeper::TxProcessingOutputOwned,
    run_batch,
    test_impl::{InMemoryPreimageSource, InMemoryTree, TxListSource},
    BatchContext, BatchOutput, ExecutionResult, InvalidTransaction, LeafProof, NextTxResponse,
    PreimageSource, ReadStorage, ReadStorageTree, StorageCommitment, TxResultCallback, TxSource,
};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal, DalError};
use zksync_mempool::L2TxFilter;
use zksync_state::{ArcOwnedStorage, BatchDiff, CommonStorage, OwnedStorage, ReadStorageFactory};
use zksync_state_keeper::{
    metrics::KEEPER_METRICS, seal_criteria::UnexecutableReason, L2BlockParams, MempoolGuard,
};
use zksync_types::{
    block::UnsealedL1BatchHeader, snapshots::SnapshotStorageLog, Address, L1BatchNumber,
    L2BlockNumber, StorageKey, StorageLog, Transaction, ERC20_TRANSFER_TOPIC, H256,
};
use zksync_types::block::L1BatchTreeData;
use zksync_vm_interface::Halt;
use zksync_zkos_vm_runner::zkos_conversions::{bytes32_to_h256, h256_to_bytes32, tx_abi_encode};

use crate::{
    batch_executor::MainBatchExecutor,
    io::{BlockParams, IoCursor, OutputHandler, StateKeeperIO},
    seal_criteria::{ConditionalSealer, IoSealCriterion, SealData, SealResolution},
    state_keeper_storage::ZkOsAsyncRocksdbCache,
    updates::{FinishedBlock, UpdatesManager},
};

const POLL_WAIT_DURATION: Duration = Duration::from_millis(100);

/// Structure used to indicate that task cancellation was requested.
#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    #[error("canceled")]
    Canceled,
    #[error(transparent)]
    Fatal(#[from] anyhow::Error),
}

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

    io: Box<dyn StateKeeperIO>,
    output_handler: OutputHandler,
    storage_factory: Arc<ZkOsAsyncRocksdbCache>,
    sealer: Arc<dyn ConditionalSealer>,
}

impl ZkosStateKeeper {
    pub fn new(
        stop_receiver: watch::Receiver<bool>,
        pool: ConnectionPool<Core>,
        io: Box<dyn StateKeeperIO>,
        output_handler: OutputHandler,
        storage_factory: Arc<ZkOsAsyncRocksdbCache>,
        sealer: Arc<dyn ConditionalSealer>,
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
            io,
            output_handler,
            storage_factory,
            sealer,
        }
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        match self.run_inner().await {
            Ok(_) => unreachable!(),
            Err(Error::Fatal(err)) => Err(err).context("state_keeper failed"),
            Err(Error::Canceled) => {
                tracing::info!("Stop signal received, state keeper is shutting down");
                Ok(())
            }
        }
    }

    async fn run_inner(mut self) -> Result<(), Error> {
        tracing::info!("Initializing ZkOs StateKeeper...");

        let mut cursor = self.io.initialize().await?;
        self.output_handler.initialize(&cursor).await?;

        if cursor.l1_batch.0 != cursor.next_l2_block.0 {
            return Err(anyhow::anyhow!(
                "For Zkos we expect batches to have just one l2 block each"
            )
            .into());
        }

        tracing::info!(
            "State root before `initialize_in_memory_storages`: {:?}",
            self.tree.storage_tree.root()
        );
        self.initialize_in_memory_storages().await?;
        tracing::info!(
            "State root after `initialize_in_memory_storages`: {:?}",
            self.tree.storage_tree.root()
        );

        while !self.is_canceled() {
            let (block_params, unsealed_header) =
                self.wait_for_new_l2_block_params(&cursor).await?;
            if let Some(unsealed_header) = unsealed_header {
                self.output_handler
                    .handle_open_batch(unsealed_header)
                    .await?;
            }

            // TODO: maybe move it wait for new l2 block params here?
            
            let mut conn = self.pool.connection_tagged("zkos_state_keeper").await.map_err(|_| Error::Fatal(anyhow::anyhow!("Failed to get connection")))?;

            let should_load_protocol_upgrade_tx = if cursor.next_l2_block.0 == 1 {
                // Genesis block
                true
            } else {
                // I am not sure if it is the right way to do it, but I will just wait until the previous block is there
                let previous_block = loop {
                    let previous_block = conn.blocks_dal().get_l2_block_header(L2BlockNumber(cursor.next_l2_block.0 - 1)).await.map_err(|_| Error::Fatal(anyhow::anyhow!("Failed to get previous block header")))?;
                    if let Some(previous_block) = previous_block {
                        break previous_block;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                };

                // For ZK OS batches we can always expect the protocol version to be present
                previous_block.protocol_version.unwrap() != block_params.protocol_version
            };

            let protocol_upgrade_tx = if should_load_protocol_upgrade_tx {
                let protocol_upgrade_tx = conn.protocol_versions_dal().get_protocol_upgrade_tx(block_params.protocol_version).await.map_err(|_| Error::Fatal(anyhow::anyhow!("Failed to get protocol upgrade tx")))?;

                protocol_upgrade_tx.map(|tx| tx.into())
            } else {
                None
            };
            
            drop(conn);

            let gas_limit = 100_000_000; // TODO: what value should be used?;
            let context = BatchContext {
                //todo: gas
                eip1559_basefee: U256::from(1000),
                //todo: gas
                native_price: U256::from(1),
                gas_per_pubdata: Default::default(),
                block_number: cursor.next_l2_block.0 as u64,
                // todo: shall we pass seconds or ms here?
                timestamp: block_params.timestamp() / 1000,
                chain_id: self.io.chain_id().as_u64(),
                gas_limit,
                coinbase: Default::default(),
                block_hashes: Default::default(),
            };

            tracing::info!("State keeper is processing block {:?} with context {:?}", cursor.next_l2_block.0, context);

            let Some(storage) = self
                .storage_factory
                .access_storage(&self.stop_receiver, cursor.l1_batch - 1)
                .await?
            else {
                return Err(Error::Canceled);
            };
            let next_enum_index = self
                .storage_factory
                .next_enum_index(cursor.l1_batch - 1)
                .await?
                .context(format!(
                    "Failed to deduct `next_enum_index` for batch #{}",
                    cursor.l1_batch
                ))?;

            let batch_executor = MainBatchExecutor::new(
                context,
                ArcOwnedStorage(Arc::new(storage)),
            );

            tracing::info!("Starting block {}", cursor.next_l2_block);

            let mut updates_manager = UpdatesManager::new(
                cursor.l1_batch,
                cursor.next_l2_block,
                block_params.timestamp_ms,
                Default::default(), //fee_account
                block_params.fee_input,
                block_params.base_fee,
                block_params.protocol_version,
                gas_limit,
            );
            let batch_output = self.run_batch(batch_executor, protocol_upgrade_tx, &mut updates_manager).await?;

            tracing::info!("Batch #{} executed successfully", cursor.l1_batch);

            // apply changes to in-memory tree/storage - note that they are not used for execution
            // todo: remove them completely from state keeper
            for storage_write in batch_output.storage_writes.iter() {
                self.tree
                    .cold_storage
                    .insert(storage_write.key, storage_write.value);
                self.tree
                    .storage_tree
                    .insert(&storage_write.key, &storage_write.value);
            }

            let root = self.tree.storage_tree.root();
            tracing::info!(
                "Batch #{} state hash from in-memory tree is {root:?}",
                cursor.l1_batch
            );

            for (hash, preimage, _) in batch_output.published_preimages.iter() {
                self.preimage_source.inner.insert(*hash, preimage.clone());
            }

            let tree_data = L1BatchTreeData {
                hash: bytes32_to_h256(*root),
                rollup_last_leaf_index: self.tree.storage_tree.next_free_slot - 1
            };


            updates_manager.final_extend(batch_output.clone(), tree_data);
            let initial_writes = self.initial_writes(&updates_manager).await?;

            self.push_block_storage_diff(
                batch_output,
                &updates_manager,
                initial_writes.clone(),
                next_enum_index,
            )
            .await;

            let finished_block = FinishedBlock {
                inner: updates_manager,
                initial_writes,
            };
            self.output_handler.handle_block(&finished_block).await?;

            cursor = IoCursor {
                next_l2_block: cursor.next_l2_block + 1,
                l1_batch: cursor.l1_batch + 1,
            };
        }
        Ok(())
    }

    async fn initialize_in_memory_storages(&mut self) -> anyhow::Result<()> {
        let mut conn = self.pool.connection_tagged("zkos_state_keeper").await?;

        let all_storage_logs = conn
            .storage_logs_dal()
            .dump_all_storage_logs_for_tests()
            .await;

        let preimages = conn
            .factory_deps_dal()
            .dump_all_factory_deps_for_tests()
            .await;

        tracing::info!(
            "Loaded from DB: {:?} storage logs and {:?} preimages",
            all_storage_logs.len(),
            preimages.len()
        );
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
            self.preimage_source
                .inner
                .insert(h256_to_bytes32(hash), value);
        }

        tracing::info!("Preimage recovery complete");
        Ok(())
    }

    async fn initial_writes(&self, updates_manager: &UpdatesManager) -> anyhow::Result<Vec<H256>> {
        let written_keys_iter = updates_manager
            .storage_logs
            .iter()
            .map(|log| log.key.hashed_key());
        let written_keys: Vec<_> = written_keys_iter.clone().collect();

        self.storage_factory
            .extract_initial_writes(&written_keys, updates_manager.l1_batch_number - 1)
            .await?
            .context(format!(
                "Failed to deduct initial writes for batch #{}",
                updates_manager.l1_batch_number
            ))
    }

    async fn push_block_storage_diff(
        &mut self,
        batch_output: BatchOutput,
        updates_manager: &UpdatesManager,
        initial_writes: Vec<H256>,
        next_enum_index: u64,
    ) {
        let state_diff = batch_output
            .storage_writes
            .into_iter()
            .map(|write| (bytes32_to_h256(write.key), bytes32_to_h256(write.value)))
            .collect();
        let factory_dep_diff = batch_output
            .published_preimages
            .into_iter()
            .map(|(hash, bytecode, _)| (bytes32_to_h256(hash), bytecode))
            .collect();

        let enum_index_diff = initial_writes
            .iter()
            .enumerate()
            .map(|(i, key)| (*key, next_enum_index + i as u64))
            .collect();

        let diff = BatchDiff {
            state_diff,
            enum_index_diff,
            factory_dep_diff,
        };
        self.storage_factory
            .push_batch_diff(updates_manager.l1_batch_number, diff)
            .await;
    }

    fn is_canceled(&self) -> bool {
        *self.stop_receiver.borrow()
    }

    async fn wait_for_new_l2_block_params(
        &mut self,
        cursor: &IoCursor,
    ) -> Result<(BlockParams, Option<UnsealedL1BatchHeader>), Error> {
        while !self.is_canceled() {
            if let (Some(params), header) = self
                .io
                .wait_for_new_l2_block_params(&cursor, POLL_WAIT_DURATION)
                .await
                .context("error waiting for new L2 block params")?
            {
                return Ok((params, header));
            }
        }
        Err(Error::Canceled)
    }

    async fn run_batch(
        &mut self,
        mut batch_executor: MainBatchExecutor,
        mut upgrade_transaction: Option<Transaction>,
        updates_manager: &mut UpdatesManager,
    ) -> Result<BatchOutput, Error> {
        let batch_output = loop {
            if self.is_canceled() && updates_manager.executed_transactions.is_empty() {
                return Err(Error::Canceled);
            }

            if self.io.should_seal_block(updates_manager) {
                if updates_manager.executed_transactions.is_empty() {
                    return Err(Error::Fatal(anyhow::anyhow!("No transactions executed, but block should be sealed")));
                }

                break batch_executor.finish_batch().await?;
            }

            let tx = if let Some(tx) = upgrade_transaction.take() {
                tx
            } else {
                let Some(tx) = self
                    .io
                    .wait_for_next_tx(POLL_WAIT_DURATION, updates_manager.timestamp)
                    .await?
                else {
                    tracing::trace!("No new transactions. Waiting!");
                    continue;
                };

                tx
            };

            let tx_hash = tx.hash();
            tracing::info!("Transaction found: {tx_hash:#?}");

            println!("tx = {:#?}", tx);
            println!("tx.execute = {:#?}", tx.execute);

            let tx_payload_encoding_size =
                zksync_protobuf::repr::encode::<zksync_dal::consensus::proto::Transaction>(&tx)
                    .len();
            let pre_execution_seal_resolution = self.sealer.should_seal_block(
                updates_manager.l2_block_number,
                updates_manager.executed_transactions.len(),
                updates_manager.gas_limit,
                &SealData {
                    gas: updates_manager.cumulative_gas_used,
                    payload_encoding_size: updates_manager.cumulative_payload_encoding_size,
                },
                &SealData {
                    gas: tx.gas_limit().as_u64(),
                    payload_encoding_size: tx_payload_encoding_size,
                },
            );
            match pre_execution_seal_resolution {
                SealResolution::Unexecutable(reason) => {
                    tracing::info!("Tx {tx_hash:#?} is unexecutable: {reason}");
                    self.io
                        .reject(&tx, UnexecutableReason::Halt(Halt::TracerCustom(reason)))
                        .await?;
                    continue;
                }
                SealResolution::Seal => {
                    break batch_executor.finish_batch().await?;
                }
                SealResolution::ExcludeAndSeal => {
                    unreachable!("Conditional sealer mustn't return ExcludeAndSeal")
                }
                SealResolution::IncludeAndSeal | SealResolution::NoSeal => {}
            }

            let tx_result = batch_executor.execute_tx(tx.clone()).await?;
            let seal_resolution_from_executor = match tx_result {
                Ok(tx_output) => {
                    updates_manager.extend_from_executed_transaction(tx.clone(), tx_output);
                    SealResolution::NoSeal
                }
                Err(reason) => {
                    // TODO: distinguish between unexecutable and exclude and seal
                    if true {
                        // TODO: map InvalidTransaction to UnexecutableReason properly.
                        SealResolution::Unexecutable(format!("{reason:?}"))
                    } else {
                        SealResolution::ExcludeAndSeal
                    }
                }
            };

            let post_execution_seal_resolution =
                seal_resolution_from_executor.stricter(pre_execution_seal_resolution);
            match post_execution_seal_resolution {
                SealResolution::Unexecutable(reason) => {
                    tracing::info!("Tx {tx_hash:#?} is unexecutable: {reason}");
                    self.io
                        .reject(&tx, UnexecutableReason::Halt(Halt::TracerCustom(reason)))
                        .await?;
                }
                SealResolution::ExcludeAndSeal => {
                    tracing::info!("Applying ExcludeAndSeal for tx {tx_hash:#?}");
                    self.io.rollback(tx).await?;

                    break batch_executor.finish_batch().await?;
                }
                SealResolution::Seal => {
                    unreachable!("SealResolution::Seal must be processed before tx execution")
                }
                SealResolution::IncludeAndSeal => {
                    break batch_executor.finish_batch().await?;
                }
                SealResolution::NoSeal => {}
            }
        };

        Ok(batch_output)
    }
}
