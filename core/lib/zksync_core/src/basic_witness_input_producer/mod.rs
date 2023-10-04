use async_trait::async_trait;
use std::rc::Rc;
use tokio::task::JoinHandle;

use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{Address, L1BatchNumber, L2ChainId, MiniblockNumber, Transaction};

use crate::state_keeper::io::common::load_pending_batch;
use crate::state_keeper::io::PendingBatchData;
use crate::state_keeper::{L1BatchExecutorBuilder, MainBatchExecutorBuilder};
use crate::sync_layer::sync_action::SyncAction::Miniblock;
use anyhow::Context;
use multivm::{VmInstance, VmInstanceData};
use std::thread;
use std::time::{Duration, Instant};
use vm::{HistoryEnabled, Vm};
use zksync_config::configs::chain::{NetworkConfig, StateKeeperConfig};
use zksync_config::{ContractsConfig, DBConfig};
use zksync_state::{PostgresStorage, PostgresStorageCaches, ReadStorage, StorageView};

// #[derive(Debug)]
// pub struct BasicWitnessInputProducerJob {
//     l1_batch_number: L1BatchNumber,
// }

pub struct BasicWitnessInputProducer {
    connection_pool: ConnectionPool,
}

fn execute_tx<S: ReadStorage>(tx: &Transaction, vm: &mut VmInstance<'_, S, HistoryEnabled>) {
    vm.make_snapshot();

    if let Ok(result) = vm.inspect_transaction_with_bytecode_compression(vec![], tx.clone(), true) {
        let compressed_bytecodes = vm.get_last_tx_compressed_bytecodes();
        vm.pop_snapshot_no_rollback();
        return;
        // return (result, compressed_bytecodes, trace);
    }

    vm.rollback_to_the_latest_snapshot();
    let result = vm
        .inspect_transaction_with_bytecode_compression(vec![], tx.clone(), false)
        .expect("Compression can't fail if we don't apply it");
    let compressed_bytecodes = vm.get_last_tx_compressed_bytecodes();
    return;
    // TODO implement tracer manager which will be responsible
    // for collecting result from all tracers and save it to the database
    // let trace = Arc::try_unwrap(call_tracer_result)
    //     .unwrap()
    //     .take()
    //     .unwrap_or_default();
    // (result, compressed_bytecodes, trace)
}

impl BasicWitnessInputProducer {
    pub fn new(connection_pool: ConnectionPool) -> Self {
        BasicWitnessInputProducer { connection_pool }
    }

    // async fn load_batch(
    //     storage: &mut StorageProcessor<'_>,
    //     current_l1_batch_number: L1BatchNumber,
    //     fee_account: Address,
    //     validation_computational_gas_limit: u32,
    //     chain_id: L2ChainId,
    // ) -> Option<PendingBatchData> {
    //     // If pending miniblock doesn't exist, it means that there is no unsynced state (i.e. no transaction
    //     // were executed after the last sealed batch).
    //     let pending_miniblock_number = {
    //         let (_, last_miniblock_number_included_in_l1_batch) = storage
    //             .blocks_dal()
    //             .get_miniblock_range_of_l1_batch(current_l1_batch_number - 1)
    //             .await
    //             .unwrap()
    //             .unwrap();
    //         last_miniblock_number_included_in_l1_batch + 1
    //     };
    //     let pending_miniblock_header = storage
    //         .blocks_dal()
    //         .get_miniblock_header(pending_miniblock_number)
    //         .await
    //         .unwrap()?;
    //
    //     tracing::info!("Getting previous batch hash");
    //     let (previous_l1_batch_hash, _) =
    //         extractors::wait_for_prev_l1_batch_params(storage, current_l1_batch_number).await;
    //
    //     tracing::info!("Getting previous miniblock hash");
    //     let prev_miniblock_hash = storage
    //         .blocks_dal()
    //         .get_miniblock_header(pending_miniblock_number - 1)
    //         .await
    //         .unwrap()
    //         .unwrap()
    //         .hash;
    //
    //     let base_system_contracts = storage
    //         .storage_dal()
    //         .get_base_system_contracts(
    //             pending_miniblock_header
    //                 .base_system_contracts_hashes
    //                 .bootloader,
    //             pending_miniblock_header
    //                 .base_system_contracts_hashes
    //                 .default_aa,
    //         )
    //         .await;
    //
    //     tracing::info!("Previous l1_batch_hash: {}", previous_l1_batch_hash);
    //     let (system_env, l1_batch_env) = l1_batch_params(
    //         current_l1_batch_number,
    //         fee_account,
    //         pending_miniblock_header.timestamp,
    //         previous_l1_batch_hash,
    //         pending_miniblock_header.l1_gas_price,
    //         pending_miniblock_header.l2_fair_gas_price,
    //         pending_miniblock_number,
    //         prev_miniblock_hash,
    //         base_system_contracts,
    //         validation_computational_gas_limit,
    //         pending_miniblock_header
    //             .protocol_version
    //             .expect("`protocol_version` must be set for pending miniblock"),
    //         pending_miniblock_header.virtual_blocks,
    //         chain_id,
    //     );
    //
    //     let pending_miniblocks = storage
    //         .transactions_dal()
    //         .get_miniblocks_to_reexecute()
    //         .await;
    //
    //     Some(PendingBatchData {
    //         l1_batch_env,
    //         system_env,
    //         pending_miniblocks,
    //     })
    // }

    pub async fn run(self) -> anyhow::Result<()> {
        let l1_batch_number = L1BatchNumber(1_u32);
        let state_keeper_config =
            StateKeeperConfig::from_env().context("StateKeeperConfig::from_env()")?;
        // let db_config = DBConfig::from_env().context("DbConfig::from_env()")?;

        let mut connection = self.connection_pool.access_storage().await?;
        let PendingBatchData {
            l1_batch_env,
            system_env,
            ..
        } = load_pending_batch(
            &mut connection,
            l1_batch_number,
            ContractsConfig::from_env()
                .context("ContractsConfig::from_env()")?
                .l2_erc20_bridge_addr,
            state_keeper_config.validation_computational_gas_limit,
            L2ChainId(
                NetworkConfig::from_env()
                    .context("NetworkConfig::from_env()")?
                    .zksync_network_id,
            ),
        )
        .await
        .unwrap();
        let connection_pool = self.connection_pool.clone();
        tokio::task::spawn_blocking(move || {
            let rt_handle = tokio::runtime::Handle::current();
            let mut connection = rt_handle
                .block_on(connection_pool.access_storage())
                .unwrap();
            let pg_storage =
                PostgresStorage::new(rt_handle.clone(), connection, MiniblockNumber(0_u32), true)
                    .with_caches(PostgresStorageCaches::new(
                        128 * 1_024 * 1_024,
                        128 * 1_024 * 1_024,
                    ));
            let storage_view = StorageView::new(pg_storage).to_rc_ptr();

            let mut instance_data =
                VmInstanceData::new(storage_view.clone(), &system_env, HistoryEnabled);

            let mut vm = VmInstance::new(l1_batch_env, system_env, &mut instance_data);
            let mut connection = rt_handle
                .block_on(connection_pool.access_storage())
                .unwrap();
            let miniblock_and_transactions = rt_handle.block_on(
                connection
                    .transactions_dal()
                    .get_miniblock_with_transactions_for_l1_batch(l1_batch_number),
            );
            println!("Then got some transactions = {miniblock_and_transactions:#?}");
            println!("got some vm, I guess");

            println!(
                "cache at start = {:#?}",
                (*storage_view).borrow().witness_block_state()
            );
            for (miniblock, txs) in miniblock_and_transactions {
                println!("Starting execution of miniblock: {miniblock:?}");
                for tx in txs {
                    execute_tx(&tx, &mut vm);
                }
                println!("Finished execution of miniblock: {miniblock:?}");
            }
            println!(
                "cache at end = {:#?}",
                (*storage_view).borrow().witness_block_state()
            );
        })
        .await
        .unwrap();
        Ok(())
    }

    // pub async fn run(self) -> anyhow::Result<()> {
    //     loop {
    //         tracing::info!("konichiwa!");
    //         thread::sleep(Duration::from_secs(5));
    //     }
    // }

    // pub async fn run(self) -> anyhow::Result<()> {
    //     let l1_batch_number = L1BatchNumber(1_u32);
    //     println!("{l1_batch_number:?}");
    //     let state_keeper_config = StateKeeperConfig::from_env().context("StateKeeperConfig::from_env()")?;
    //     let db_config = DBConfig::from_env().context("DbConfig::from_env()")?;
    //     let mut storage = self.connection_pool.access_storage().await?;
    //     let PendingBatchData {
    //         l1_batch_env, system_env, ..
    //     } = load_pending_batch(
    //         &mut storage,
    //         l1_batch_number,
    //         ContractsConfig::from_env().context("ContractsConfig::from_env()")?.l2_erc20_bridge_addr,
    //         state_keeper_config.validation_computational_gas_limit,
    //         L2ChainId(NetworkConfig::from_env().context("NetworkConfig::from_env()")?.zksync_network_id),
    //     ).await.unwrap();
    //     println!("L1BatchEnv = {l1_batch_env:#?}");
    //     println!("SystemEnv = {system_env:#?}");
    //     println!("got here, I guess");
    //
    //     let batch_executor_base = MainBatchExecutorBuilder::new(
    //         db_config.state_keeper_db_path.clone(),
    //         self.connection_pool.clone(),
    //         state_keeper_config.max_allowed_l2_tx_gas_limit.into(),
    //         state_keeper_config.save_call_traces,
    //         state_keeper_config.upload_witness_inputs_to_gcs,
    //     );
    //     println!("trouble here?");
    //     let batch_executor = batch_executor_base.init_batch(l1_batch_env.clone(), system_env.clone()).await;
    //     println!("{batch_executor:?}");
    //     thread::sleep(Duration::from_secs(100));
    //     Ok(())
    //     // loop {
    //     //     tracing::info!("konichiwa!");
    //     //     thread::sleep(Duration::from_secs(5));
    //     // }
    // }
}

// impl BasicWitnessInputProducer {
//     async fn process_job_impl(&self) -> anyhow::Result<Self::JobArtifacts>{
//         Ok(())
//     }
// }
//
//
// #[async_trait]
// impl JobProcessor for BasicWitnessInputProducer {
//     type Job = BasicWitnessInputProducerJob;
//     type JobId = L1BatchNumber;
//     type JobArtifacts = ();
//     const SERVICE_NAME: &'static str = "";
//
//     async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
//         tracing::info!("EMIL -- get_next_job()");
//         let mut connection = self
//             .connection_pool
//             .access_storage()
//             .await
//             .expect("couldn't get a connection from the pool");
//         let l1_batch_to_process = connection
//             .basic_witness_input_producer_dal()
//             .get_next_basic_witness_input_producer_job()
//             .await;
//         match l1_batch_to_process {
//             Some(val) => Ok(Some((
//                 val,
//                 BasicWitnessInputProducerJob {
//                     l1_batch_number: val,
//                 },
//             ))),
//             None => Ok(None),
//         }
//     }
//
//     async fn save_failure(&self, job_id: Self::JobId, started_at: Instant, error: String) {
//         tracing::info!("EMIL -- save_failure({job_id:?}; {started_at:?}; {error:?})");
//         thread::sleep(Duration::from_secs(10));
//     }
//
//     async fn process_job(
//         &self,
//         job: Self::Job,
//         started_at: Instant,
//     ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
//         tracing::info!("EMIL -- process_job({job:?}; {started_at:?})");
//         thread::sleep(Duration::from_secs(10));
//         tokio::spawn(self.process_job_impl())
//     }
//
//     async fn save_result(
//         &self,
//         job_id: Self::JobId,
//         started_at: Instant,
//         artifacts: Self::JobArtifacts,
//     ) -> anyhow::Result<()> {
//         tracing::info!("EMIL -- save_result({job_id:?}; {started_at:?}; {artifacts:?})");
//         thread::sleep(Duration::from_secs(10));
//         Ok(())
//     }
// }
