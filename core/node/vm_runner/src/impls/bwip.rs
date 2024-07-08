use std::{collections::HashSet, sync::Arc};

use anyhow::anyhow;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::inputs::VMRunWitnessInputData;
use zksync_state_keeper::{MainBatchExecutor, StateKeeperOutputHandler, UpdatesManager};
use zksync_types::{
    block::StorageOracleInfo, witness_block_state::WitnessStorageState, L1BatchNumber, L2ChainId,
    H256,
};
use zksync_utils::{bytes_to_chunks, h256_to_u256, u256_to_h256};

use crate::{
    storage::StorageSyncTask, ConcurrentOutputHandlerFactory, ConcurrentOutputHandlerFactoryTask,
    OutputHandlerFactory, VmRunner, VmRunnerIo, VmRunnerStorage,
};

/// A standalone component that retrieves all needed data for basic witness generation and saves it to the bucket
#[derive(Debug)]
pub struct BasicWitnessInputProducer {
    vm_runner: VmRunner,
}

impl BasicWitnessInputProducer {
    /// Create a new BWIP from the provided DB parameters and window size which
    /// regulates how many batches this component can handle at the same time.
    pub async fn new(
        pool: ConnectionPool<Core>,
        object_store: Arc<dyn ObjectStore>,
        rocksdb_path: String,
        chain_id: L2ChainId,
        first_processed_batch: L1BatchNumber,
        window_size: u32,
    ) -> anyhow::Result<(Self, BasicWitnessInputProducerTasks)> {
        let io = BasicWitnessInputProducerIo {
            first_processed_batch,
            window_size,
        };
        let (loader, loader_task) =
            VmRunnerStorage::new(pool.clone(), rocksdb_path, io.clone(), chain_id).await?;
        let output_handler_factory = BasicWitnessInputProducerOutputHandlerFactory {
            pool: pool.clone(),
            object_store,
        };
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

    /// Continuously loads new available batches and writes the corresponding data
    /// produced by that batch.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    pub async fn run(self, stop_receiver: &watch::Receiver<bool>) -> anyhow::Result<()> {
        self.vm_runner.run(stop_receiver).await
    }
}

/// A collections of tasks that need to be run in order for BWIP to work as
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
    first_processed_batch: L1BatchNumber,
    window_size: u32,
}

#[async_trait]
impl VmRunnerIo for BasicWitnessInputProducerIo {
    fn name(&self) -> &'static str {
        "basic_witness_input_producer"
    }

    async fn latest_processed_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(conn
            .vm_runner_dal()
            .get_bwip_latest_processed_batch()
            .await?
            .unwrap_or(self.first_processed_batch))
    }

    async fn last_ready_to_be_loaded_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(conn
            .vm_runner_dal()
            .get_bwip_last_ready_batch(self.first_processed_batch, self.window_size)
            .await?)
    }

    async fn mark_l1_batch_as_processing(
        &self,
        conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        Ok(conn
            .vm_runner_dal()
            .mark_bwip_batch_as_processing(l1_batch_number)
            .await?)
    }

    async fn mark_l1_batch_as_completed(
        &self,
        conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        conn.vm_runner_dal()
            .mark_bwip_batch_as_completed(l1_batch_number)
            .await
    }
}

#[derive(Debug)]
struct BasicWitnessInputProducerOutputHandler {
    pool: ConnectionPool<Core>,
    object_store: Arc<dyn ObjectStore>,
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
        let l1_batch_number = updates_manager.l1_batch.number;
        let mut connection = self.pool.connection().await?;

        tracing::info!(%l1_batch_number, "Started saving VM run data");

        let result =
            get_updates_manager_witness_input_data(&mut connection, updates_manager).await?;

        assert_database_witness_input_data(&mut connection, l1_batch_number, &result).await;

        let blob_url = self.object_store.put(l1_batch_number, &result).await?;

        tracing::info!(%l1_batch_number, "Saved VM run data");

        connection
            .proof_generation_dal()
            .save_vm_runner_artifacts_metadata(l1_batch_number, &blob_url)
            .await?;

        Ok(())
    }
}

async fn get_updates_manager_witness_input_data(
    connection: &mut Connection<'_, Core>,
    updates_manager: Arc<UpdatesManager>,
) -> anyhow::Result<VMRunWitnessInputData> {
    let l1_batch_number = updates_manager.l1_batch.number;
    let finished_batch = updates_manager
        .l1_batch
        .finished
        .clone()
        .ok_or_else(|| anyhow!("L1 batch {l1_batch_number:?} is not finished"))?;

    let initial_heap_content = finished_batch.final_bootloader_memory.unwrap(); // might be just empty
    let default_aa = updates_manager.base_system_contract_hashes().default_aa;
    let bootloader = updates_manager.base_system_contract_hashes().bootloader;
    let bootloader_code_bytes = connection
        .factory_deps_dal()
        .get_sealed_factory_dep(bootloader)
        .await?
        .ok_or_else(|| anyhow!("Failed fetching bootloader bytecode from DB"))?;
    let bootloader_code = bytes_to_chunks(&bootloader_code_bytes);

    let account_code_hash = h256_to_u256(default_aa);
    let account_bytecode_bytes = connection
        .factory_deps_dal()
        .get_sealed_factory_dep(default_aa)
        .await?
        .ok_or_else(|| anyhow!("Default account bytecode should exist"))?;
    let account_bytecode = bytes_to_chunks(&account_bytecode_bytes);

    let hashes: HashSet<H256> = finished_batch
        .final_execution_state
        .used_contract_hashes
        .iter()
        // SMA-1555: remove this hack once updated to the latest version of `zkevm_test_harness`
        .filter(|&&hash| hash != h256_to_u256(bootloader))
        .map(|hash| u256_to_h256(*hash))
        .collect();
    let mut used_bytecodes = connection
        .factory_deps_dal()
        .get_factory_deps(&hashes)
        .await;
    if finished_batch
        .final_execution_state
        .used_contract_hashes
        .contains(&account_code_hash)
    {
        used_bytecodes.insert(account_code_hash, account_bytecode);
    }

    let storage_refunds = finished_batch.final_execution_state.storage_refunds;
    let pubdata_costs = finished_batch.final_execution_state.pubdata_costs;

    let storage_view_cache = updates_manager
        .storage_view_cache()
        .expect("Storage view cache was not initialized");

    let witness_block_state = WitnessStorageState {
        read_storage_key: storage_view_cache.read_storage_keys(),
        is_write_initial: storage_view_cache.initial_writes(),
    };

    Ok(VMRunWitnessInputData {
        l1_batch_number,
        used_bytecodes,
        initial_heap_content,

        protocol_version: updates_manager.protocol_version(),

        bootloader_code,
        default_account_code_hash: account_code_hash,
        storage_refunds,
        pubdata_costs,
        witness_block_state,
    })
}

async fn assert_database_witness_input_data(
    connection: &mut Connection<'_, Core>,
    l1_batch_number: L1BatchNumber,
    result: &VMRunWitnessInputData,
) {
    let block_header = connection
        .blocks_dal()
        .get_l1_batch_header(l1_batch_number)
        .await
        .expect("Failed fetching L1 block from DB")
        .expect("L1 block header should exist");

    let initial_heap_content = connection
        .blocks_dal()
        .get_initial_bootloader_heap(l1_batch_number)
        .await
        .expect("Failed fetching initial heap content from DB")
        .expect("Initial bootloader heap should exist");

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
        .filter(|&&hash| hash != h256_to_u256(block_header.base_system_contracts_hashes.bootloader))
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

    let StorageOracleInfo {
        storage_refunds,
        pubdata_costs,
    } = connection
        .blocks_dal()
        .get_storage_oracle_info(block_header.number)
        .await
        .expect("Failed fetching L1 block from DB")
        .expect("Storage oracle info should exist");
    let pubdata_costs = pubdata_costs.unwrap();

    let bootloader_code_bytes = connection
        .factory_deps_dal()
        .get_sealed_factory_dep(block_header.base_system_contracts_hashes.bootloader)
        .await
        .expect("Failed fetching bootloader bytecode from DB")
        .expect("Bootloader bytecode should exist");
    let bootloader_code = bytes_to_chunks(&bootloader_code_bytes);

    assert_eq!(
        block_header.protocol_version.unwrap(),
        result.protocol_version,
        "Protocol version mismatch in basic witness input producer: DB: {:?}, UpdatesManager: {:?}",
        block_header.protocol_version,
        result.protocol_version
    );
    assert_eq!(
        used_bytecodes, result.used_bytecodes,
        "Used bytecodes mismatch in basic witness input producer: DB: {:?}, UpdatesManager: {:?}",
        used_bytecodes, result.used_bytecodes
    );
    assert_eq!(
        storage_refunds, result.storage_refunds,
        "Storage refunds mismatch in basic witness input producer: DB: {:?}, UpdatesManager: {:?}",
        storage_refunds, result.storage_refunds
    );
    assert_eq!(
        pubdata_costs, result.pubdata_costs,
        "Pubdata costs mismatch in basic witness input producer: DB: {:?}, UpdatesManager: {:?}",
        pubdata_costs, result.pubdata_costs
    );
    assert_eq!(
        initial_heap_content, result.initial_heap_content,
        "Initial heap content mismatch in basic witness input producer: DB: {:?}, UpdatesManager: {:?}",
        initial_heap_content, result.initial_heap_content
    );
    assert_eq!(
        bootloader_code, result.bootloader_code,
        "Bootloader code mismatch in basic witness input producer: DB: {:?}, UpdatesManager: {:?}",
        bootloader_code, result.bootloader_code
    );
    assert_eq!(
        account_code_hash, result.default_account_code_hash,
        "Default account code hash mismatch in basic witness input producer: DB: {:?}, UpdatesManager: {:?}",
        account_code_hash, result.default_account_code_hash
    );
}

#[derive(Debug)]
struct BasicWitnessInputProducerOutputHandlerFactory {
    pool: ConnectionPool<Core>,
    object_store: Arc<dyn ObjectStore>,
}

#[async_trait]
impl OutputHandlerFactory for BasicWitnessInputProducerOutputHandlerFactory {
    async fn create_handler(
        &mut self,
        _l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Box<dyn StateKeeperOutputHandler>> {
        Ok(Box::new(BasicWitnessInputProducerOutputHandler {
            pool: self.pool.clone(),
            object_store: self.object_store.clone(),
        }))
    }
}
