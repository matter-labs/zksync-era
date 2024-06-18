use std::{collections::HashSet, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_prover_interface::inputs::WitnessGeneratorData;
use zksync_state_keeper::{MainBatchExecutor, StateKeeperOutputHandler, UpdatesManager};
use zksync_types::{L1BatchNumber, L2ChainId, ProtocolVersionId, H256};
use zksync_utils::{bytes_to_chunks, h256_to_u256, u256_to_h256};

use crate::{
    storage::StorageSyncTask, ConcurrentOutputHandlerFactory, ConcurrentOutputHandlerFactoryTask,
    OutputHandlerFactory, VmRunner, VmRunnerIo, VmRunnerStorage,
};

/// A standalone component that writes protective reads asynchronously to state keeper.
#[derive(Debug)]
pub struct BasicWitnessInputProducer {
    vm_runner: VmRunner,
}

impl BasicWitnessInputProducer {
    /// Create a new protective reads writer from the provided DB parameters and window size which
    /// regulates how many batches this component can handle at the same time.
    pub async fn new(
        pool: ConnectionPool<Core>,
        rocksdb_path: String,
        chain_id: L2ChainId,
        window_size: u32,
    ) -> anyhow::Result<(Self, BasicWitnessInputProducerTasks)> {
        let io = BasicWitnessInputProducerIo { window_size };
        let (loader, loader_task) =
            VmRunnerStorage::new(pool.clone(), rocksdb_path, io.clone(), chain_id).await?;
        let output_handler_factory =
            BasicWitnessInputProducerOutputHandlerFactory { pool: pool.clone() };
        let (output_handler_factory, output_handler_factory_task) =
            ConcurrentOutputHandlerFactory::new(pool.clone(), io.clone(), output_handler_factory);
        let batch_processor = MainBatchExecutor::new(false, false);
        let vm_runner = VmRunner::new(
            pool,
            Box::new(io),
            Arc::new(loader),
            Box::new(output_handler_factory),
            Box::new(batch_processor),
        );
        Ok((
            Self { vm_runner },
            BasicWitnessInputProducerTasks {
                loader_task,
                output_handler_factory_task,
            },
        ))
    }

    /// Continuously loads new available batches and writes the corresponding protective reads
    /// produced by that batch.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    pub async fn run(self, stop_receiver: &watch::Receiver<bool>) -> anyhow::Result<()> {
        self.vm_runner.run(stop_receiver).await
    }
}

/// A collections of tasks that need to be run in order for protective reads writer to work as
/// intended.
#[derive(Debug)]
pub struct BasicWitnessInputProducerTasks {
    /// Task that synchronizes storage with new available batches.
    pub loader_task: StorageSyncTask<BasicWitnessInputProducerIo>,
    /// Task that handles output from processed batches.
    pub output_handler_factory_task:
        ConcurrentOutputHandlerFactoryTask<BasicWitnessInputProducerIo>,
}

#[derive(Debug, Clone)]
pub struct BasicWitnessInputProducerIo {
    window_size: u32,
}

#[async_trait]
impl VmRunnerIo for BasicWitnessInputProducerIo {
    fn name(&self) -> &'static str {
        "protective_reads_writer"
    }

    async fn latest_processed_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(conn
            .vm_runner_dal()
            .get_protective_reads_latest_processed_batch()
            .await?)
    }

    async fn last_ready_to_be_loaded_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(conn
            .vm_runner_dal()
            .get_protective_reads_last_ready_batch(self.window_size)
            .await?)
    }

    async fn mark_l1_batch_as_completed(
        &self,
        conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        Ok(conn
            .vm_runner_dal()
            .mark_protective_reads_batch_as_completed(l1_batch_number)
            .await?)
    }
}

#[derive(Debug)]
struct BasicWitnessInputProducerOutputHandler {
    pool: ConnectionPool<Core>,
}

#[async_trait]
impl StateKeeperOutputHandler for BasicWitnessInputProducerOutputHandler {
    async fn handle_l2_block(&mut self, _updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        Ok(())
    }

    async fn handle_l1_batch(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        let finished_batch = updates_manager
            .l1_batch
            .finished
            .as_ref()
            .context("L1 batch is not actually finished")?;
        let l1_batch_number = updates_manager.l1_batch.number;

        let mut connection = self.pool.connection().await?;

        let block_header = connection
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .unwrap()
            .unwrap();

        let initial_heap_content = connection
            .blocks_dal()
            .get_initial_bootloader_heap(l1_batch_number)
            .await
            .unwrap()
            .unwrap();

        let (_, previous_block_timestamp) = connection
            .blocks_dal()
            .get_l1_batch_state_root_and_timestamp(l1_batch_number - 1)
            .await
            .unwrap()
            .unwrap();
        let previous_block_hash = connection
            .blocks_dal()
            .get_l1_batch_state_root(l1_batch_number - 1)
            .await
            .unwrap()
            .expect("cannot generate witness before the root hash is computed");

        let account_code_hash = h256_to_u256(block_header.base_system_contracts_hashes.default_aa);
        let account_bytecode_bytes = connection
            .factory_deps_dal()
            .get_sealed_factory_dep(block_header.base_system_contracts_hashes.default_aa)
            .await
            .expect("Failed fetching default account bytecode from DB")
            .expect("Default account bytecode should exist");
        let account_bytecode = bytes_to_chunks(&account_bytecode_bytes);

        let hashes: HashSet<H256> = block_header
            .used_contract_hashes
            .iter()
            // SMA-1555: remove this hack once updated to the latest version of `zkevm_test_harness`
            .filter(|&&hash| {
                hash != h256_to_u256(block_header.base_system_contracts_hashes.bootloader)
            })
            .map(|hash| u256_to_h256(*hash))
            .collect();
        let mut used_bytecodes = connection
            .factory_deps_dal()
            .get_factory_deps(&hashes)
            .await;
        if block_header
            .used_contract_hashes
            .contains(&account_code_hash)
        {
            used_bytecodes.insert(account_code_hash, account_bytecode);
        }

        assert_eq!(
            hashes.len(),
            used_bytecodes.len(),
            "{} factory deps are not found in DB",
            hashes.len() - used_bytecodes.len()
        );

        let result = WitnessGeneratorData {
            block_number: l1_batch_number,
            previous_batch_with_metadata: L1BatchWithMetadata {},
            last_miniblock_number: Default::default(),
            previous_block_hash,
            previous_block_timestamp,
            block_timestamp: block_header.timestamp,
            used_bytecodes,
            initial_heap_content,

            protocol_version: block_header
                .protocol_version
                .unwrap_or(ProtocolVersionId::last_potentially_undefined()),
            storage_logs: vec![],
            bootloader_code_hash: Default::default(),
            bootloader_code: vec![],
            default_account_code_hash: account_code_hash,
            storage_refunds: vec![],
            pubdata_costs: None,
            witness_storage_memory: (),
            merkle_paths_input: (),
        };

        Ok(())
    }
}

#[derive(Debug)]
struct BasicWitnessInputProducerOutputHandlerFactory {
    pool: ConnectionPool<Core>,
}

#[async_trait]
impl OutputHandlerFactory for BasicWitnessInputProducerOutputHandlerFactory {
    async fn create_handler(
        &mut self,
        _l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Box<dyn StateKeeperOutputHandler>> {
        Ok(Box::new(BasicWitnessInputProducerOutputHandler {
            pool: self.pool.clone(),
        }))
    }
}

pub struct BasicCircuitWitnessGeneratorInput {}
