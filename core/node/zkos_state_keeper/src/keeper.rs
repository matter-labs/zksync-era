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
    common_structs::derive_flat_storage_key,
    system::{system_io_oracle::PreimageType, ExecutionEnvironmentType},
    utils::Bytes32,
};
use zk_os_basic_system::{
    basic_io_implementer::{
        address_into_special_storage_key, io_implementer::NOMINAL_TOKEN_BALANCE_STORAGE_ADDRESS,
    },
    basic_system::simple_growable_storage::TestingTree,
};
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
    io::IoCursor, metrics::KEEPER_METRICS, seal_criteria::UnexecutableReason, L2BlockParams,
    MempoolGuard,
};
use zksync_types::{
    block::UnsealedL1BatchHeader, snapshots::SnapshotStorageLog, Address, L1BatchNumber,
    L2BlockNumber, StorageKey, StorageLog, Transaction, ERC20_TRANSFER_TOPIC, H256,
};
use zksync_vm_interface::Halt;
use zksync_zkos_vm_runner::zkos_conversions::{bytes32_to_h256, h256_to_bytes32, tx_abi_encode};

use crate::{
    io::{BlockParams, OutputHandler, StateKeeperIO},
    millis_since_epoch,
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
}

impl ZkosStateKeeper {
    pub fn new(
        stop_receiver: watch::Receiver<bool>,
        pool: ConnectionPool<Core>,
        io: Box<dyn StateKeeperIO>,
        output_handler: OutputHandler,
        storage_factory: Arc<ZkOsAsyncRocksdbCache>,
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

        // let mut connection = self
        //     .pool
        //     .connection_tagged("state_keeper")
        //     .await
        //     .map_err(DalError::generalize)?;
        // Self::fund_dev_wallets_if_needed(&mut connection, cursor.next_l2_block).await;
        // drop(connection);

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

            let (sender, receiver) = tokio::sync::mpsc::channel(1);
            let (result_sender, result_receiver) = tokio::sync::mpsc::channel(1);
            let tx_source = OnlineTxSource::new(receiver);
            let tx_callback = ChannelTxResultCallback::new(result_sender);

            let gas_limit = 100_000_000; // TODO: what value should be used?;
            let tx_processor = TxProcessor::new(
                sender,
                result_receiver,
                10,
                block_params.timestamp,
                gas_limit,
            );

            let context = BatchContext {
                //todo: gas
                eip1559_basefee: U256::from(block_params.base_fee),
                gas_per_pubdata: Default::default(),
                block_number: cursor.next_l2_block.0 as u64,
                timestamp: block_params.timestamp,
                chain_id: self.io.chain_id().as_u64(),
                gas_limit,
                coinbase: Default::default(),
                block_hashes: Default::default(),
            };

            let storage_commitment = StorageCommitment {
                root: self.tree.storage_tree.root().clone(),
                next_free_slot: self.tree.storage_tree.next_free_slot,
            };
            tracing::info!(
                "Starting block {} with commitment root {:?} and next_free_slot {:?}",
                cursor.next_l2_block,
                storage_commitment.root,
                storage_commitment.next_free_slot
            );

            //todo: at least use refcell instead of cloning
            tracing::info!("Cloning in-memory storages for the batch");
            let tree = self.tree.clone();
            let preimage_source = self.preimage_source.clone();
            tracing::info!("Cloning done, running batch");

            let Some(storage) = self
                .storage_factory
                .access_storage(&self.stop_receiver, cursor.l1_batch - 1)
                .await?
            else {
                return Err(Error::Canceled);
            };
            let storage = Arc::new(storage);
            let run_batch_task = spawn_blocking(move || {
                run_batch(
                    context,
                    storage_commitment,
                    ArcOwnedStorage(storage.clone()),
                    ArcOwnedStorage(storage),
                    tx_source,
                    tx_callback,
                )
            });

            let mut updates_manager = UpdatesManager::new(
                cursor.l1_batch,
                cursor.next_l2_block,
                block_params.timestamp,
                Default::default(), //fee_account
                block_params.fee_input,
                block_params.base_fee,
                block_params.protocol_version,
                gas_limit,
            );

            let executed_transactions = tx_processor.run(&mut self.io, &self.stop_receiver).await?;

            let result = run_batch_task
                .await
                .expect("Task run_batch panicked")
                .map_err(|err| anyhow::anyhow!(err.0))
                .context("run_batch failed")?;

            tracing::info!("Batch #{} executed successfully", cursor.l1_batch);

            for storage_write in result.storage_writes.iter() {
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

            for (hash, preimage) in result.published_preimages.iter() {
                self.preimage_source.inner.insert(
                    (PreimageType::Bytecode(ExecutionEnvironmentType::EVM), *hash),
                    preimage.clone(),
                );
            }

            updates_manager.extend(executed_transactions, result.clone());
            let (initial_writes, next_enum_index) = self.initial_writes(&updates_manager).await?;

            self.push_block_storage_diff(
                result,
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
                prev_l2_block_hash: Default::default(),
                prev_l2_block_timestamp: block_params.timestamp,
                l1_batch: cursor.l1_batch + 1,
            };
        }
        Ok(())
    }

    // Funds dev wallets with some ETH for testing
    // only funds wallets that were not funded before
    // wallets can be added to this list without regenesis
    async fn fund_dev_wallets_if_needed(
        connection: &mut Connection<'_, Core>,
        pending_block_number: L2BlockNumber,
    ) {
        for address in &[
            "0x27FBEc0B5D2A2B89f77e4D3648bBBBCF11784bdE",
            "0x2eF0972bd8AFc29d63b2412508ce5e20219b9A8c",
            "0xBC989fDe9e54cAd2aB4392Af6dF60f04873A033A",
        ] {
            let address = B160::from_str(address).unwrap();
            let key = address_into_special_storage_key(&address);
            let balance = bytes32_to_h256(Bytes32::from_u256_be(
                U256::from_str("1700000000000000000").unwrap(),
            ));
            let flat_key = bytes32_to_h256(derive_flat_storage_key(
                &NOMINAL_TOKEN_BALANCE_STORAGE_ADDRESS,
                &key,
            ));

            let r = connection
                .storage_logs_dal()
                .get_storage_values(&[flat_key], pending_block_number)
                .await
                .expect("Failed to get storage values for initial balances");
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
                .insert_storage_logs_from_snapshot(L2BlockNumber(0), &logs)
                .await
                .expect("Failed to insert storage logs for initial balances");
        }
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

    async fn initial_writes(
        &self,
        updates_manager: &UpdatesManager,
    ) -> anyhow::Result<(Vec<H256>, u64)> {
        let mut connection = self.pool.connection_tagged("state_keeper").await?;

        let initial_writes: Vec<_> = {
            let deduplicated_writes_hashed_keys_iter = updates_manager
                .storage_logs
                .iter()
                .map(|log| log.key.hashed_key());
            let deduplicated_writes_hashed_keys: Vec<_> =
                deduplicated_writes_hashed_keys_iter.clone().collect();
            let non_initial_writes = connection
                .storage_logs_dedup_dal()
                .filter_written_slots(&deduplicated_writes_hashed_keys)
                .await?;
            deduplicated_writes_hashed_keys_iter
                .filter(|hashed_key| !non_initial_writes.contains(hashed_key))
                .collect()
        };
        let next_enum_index = connection
            .storage_logs_dedup_dal()
            .max_enumeration_index_by_l1_batch(updates_manager.l1_batch_number - 1)
            .await
            .map_err(DalError::generalize)?
            .context("missing enum index in DB")?
            + 1;

        Ok((initial_writes, next_enum_index))
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
            .map(|(hash, bytecode)| (bytes32_to_h256(hash), bytecode))
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
}

#[derive(Debug)]
pub struct OnlineTxSource {
    receiver: Receiver<NextTxResponse>,
}

impl OnlineTxSource {
    pub fn new(receiver: Receiver<NextTxResponse>) -> Self {
        Self { receiver }
    }
}

impl TxSource for OnlineTxSource {
    fn get_next_tx(&mut self) -> NextTxResponse {
        self.receiver.blocking_recv().unwrap_or_else(|| {
            // Sender shouldn't be dropped i.e. we always try to finish block execution properly
            // on shutdown request. This looks reasonable given block will take short period of time
            // (most likely 1 second), and if there are no transactions we can still close an empty block
            // without persisting it.
            panic!("next tx sender was dropped without yielding `SealBatch`")
        })
    }
}

#[derive(Debug)]
pub struct ChannelTxResultCallback {
    sender: Sender<Result<TxProcessingOutputOwned, InvalidTransaction>>,
}

impl ChannelTxResultCallback {
    pub fn new(sender: Sender<Result<TxProcessingOutputOwned, InvalidTransaction>>) -> Self {
        Self { sender }
    }
}

impl TxResultCallback for ChannelTxResultCallback {
    fn tx_executed(
        &mut self,
        tx_execution_result: Result<TxProcessingOutputOwned, InvalidTransaction>,
    ) {
        self.sender
            .blocking_send(tx_execution_result)
            .expect("Tx execution result receiver was dropped");
    }
}

/// Struct that
/// - Polls the mempool
/// - Sends transaction over channel to the execution thread
/// - Keeps track of seal criteria: currently there are 3 of them: timestamp, gas, and number of txs.
#[derive(Debug)]
pub struct TxProcessor {
    sender: Sender<NextTxResponse>,
    result_receiver: Receiver<Result<TxProcessingOutputOwned, InvalidTransaction>>,
    max_tx_count: usize,
    block_timestamp: u64,
    block_gas_limit: u64,
}

impl TxProcessor {
    pub fn new(
        sender: Sender<NextTxResponse>,
        result_receiver: Receiver<Result<TxProcessingOutputOwned, InvalidTransaction>>,
        max_tx_count: usize,
        block_timestamp: u64,
        block_gas_limit: u64,
    ) -> Self {
        Self {
            sender,
            result_receiver,
            max_tx_count,
            block_timestamp,
            block_gas_limit,
        }
    }

    pub async fn run(
        mut self,
        io: &mut Box<dyn StateKeeperIO>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<Vec<Transaction>, Error> {
        let mut executed_txs = Vec::new();
        let mut gas_used = 0;

        let started_at = Instant::now();
        loop {
            if *stop_receiver.borrow() && executed_txs.is_empty() {
                return Err(Error::Canceled);
            }

            // TODO: proper timeout, should it be at `block_started_ms + 1000ms` or `block_timestamp_s + 1s`?
            if started_at.elapsed() > Duration::from_secs(1) {
                if !executed_txs.is_empty() {
                    self.sender
                        .send(NextTxResponse::SealBatch)
                        .await
                        .context("NextTxResponse receiver was dropped")?;
                    break;
                }
            }

            let Some(tx) = io
                .wait_for_next_tx(POLL_WAIT_DURATION, self.block_timestamp)
                .await?
            else {
                tracing::trace!("No new transactions. Waiting!");
                continue;
            };

            if !self.block_has_capacity_for_tx(
                executed_txs.len(),
                gas_used,
                tx.gas_limit().as_u64(),
            ) {
                // Exclude and seal
                self.sender
                    .send(NextTxResponse::SealBatch)
                    .await
                    .context("NextTxResponse receiver was dropped")?;

                io.rollback(tx).await?;

                break;
            }

            tracing::info!("Transaction found: {:?}", tx);
            let encoded = tx_abi_encode(tx.clone());

            self.sender
                .send(NextTxResponse::Tx(encoded))
                .await
                .context("NextTxResponse receiver was dropped")?;

            let tx_result = self
                .result_receiver
                .recv()
                .await
                .context("Result sender was dropped")?;
            match tx_result {
                Ok(tx_output) => {
                    executed_txs.push(tx);
                    gas_used += tx_output.gas_used;
                }
                Err(reason) => {
                    let tx_hash = tx.hash();
                    // TODO: map InvalidTransaction to UnexecutableReason properly.
                    let reason = UnexecutableReason::Halt(Halt::TracerCustom(format!(
                        "Invalid transaction, hash: {tx_hash:#?}, reason: {reason:?}"
                    )));
                    io.reject(&tx, reason).await?;
                }
            }
        }

        Ok(executed_txs)
    }

    fn block_has_capacity_for_tx(
        &self,
        tx_count: usize,
        block_gas_used: u64,
        tx_gas_limit: u64,
    ) -> bool {
        if tx_count >= self.max_tx_count {
            tracing::info!("Block sealed by max_tx_count");
            return false;
        }

        if block_gas_used + tx_gas_limit > self.block_gas_limit {
            tracing::info!("Block sealed by gas");
            return false;
        }

        true
    }
}
