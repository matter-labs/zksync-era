use std::{fs, sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use futures::TryFutureExt;
use multivm::interface::{L2BlockEnv, VmInterface};
use tokio::{runtime::Handle, task::JoinHandle};
use vm_utils::{create_vm, execute_tx};
use zksync_basic_types::U256;
use zksync_crypto::hasher::{blake2::Blake2Hasher, Hasher};
use zksync_dal::{basic_witness_input_producer_dal::JOB_MAX_ATTEMPT, ConnectionPool};
use zksync_merkle_tree::{
    BlockOutputWithProofs, TreeInstruction, TreeLogEntry, TreeLogEntryWithProof,
};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_interface::inputs::PrepareBasicCircuitsJob;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    ethabi::ethereum_types::BigEndianHash, witness_block_state::WitnessBlockState, AccountTreeId,
    Address, L1BatchNumber, L2ChainId, StorageKey,
};

use self::metrics::METRICS;

mod metrics;
/// Execute batches in TEE to verify their results.
#[derive(Debug)]
pub struct Sgx2fa {
    connection_pool: ConnectionPool,
    l2_chain_id: L2ChainId,
    object_store: Arc<dyn ObjectStore>,
}

impl Sgx2fa {
    pub async fn new(
        connection_pool: ConnectionPool,
        store_factory: &ObjectStoreFactory,
        l2_chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        Ok(Sgx2fa {
            connection_pool,
            object_store: store_factory.create_store().await,
            l2_chain_id,
        })
    }

    fn process_job_impl(
        rt_handle: Handle,
        l1_batch_number: L1BatchNumber,
        started_at: Instant,
        connection_pool: ConnectionPool,
        l2_chain_id: L2ChainId,
        object_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<WitnessBlockState> {
        let mut connection = rt_handle
            .block_on(connection_pool.access_storage())
            .context("failed to get connection for BasicWitnessInputProducer")?;

        let miniblocks_execution_data = rt_handle.block_on(
            connection
                .transactions_dal()
                .get_miniblocks_to_execute_for_l1_batch(l1_batch_number),
        )?;

        let job = rt_handle
            .block_on(object_store.get::<PrepareBasicCircuitsJob>(l1_batch_number))
            .unwrap();
        let wbs = rt_handle
            .block_on(object_store.get::<WitnessBlockState>(l1_batch_number))
            .unwrap();
        //tracing::debug!("{:?}", job.into_merkle_paths().next());
        tracing::info!("=== Job ===");
        //tracing::info!("{:#X?}", job);
        tracing::info!("+++ Job +++");

        // pub struct StorageLogMetadata {
        //     #[serde_as(as = "Bytes")]
        //     pub root_hash: [u8; HASH_LEN],
        //     pub is_write: bool,
        //     pub first_write: bool,
        //     #[serde_as(as = "Vec<Bytes>")]
        //     pub merkle_paths: Vec<[u8; HASH_LEN]>,
        //     pub leaf_hashed_key: U256,
        //     pub leaf_enumeration_index: u64,
        //     // **NB.** For compatibility reasons, `#[serde_as(as = "Bytes")]` attributes are not added below.
        //     pub value_written: [u8; HASH_LEN],
        //     pub value_read: [u8; HASH_LEN],
        // }

        // Four cases:
        // Uninit read
        // read
        // update
        // (initial) write

        tracing::info!(
            "# entries in PrepareBasicCircuitsJob={:?}",
            job.clone().into_merkle_paths().len()
        );

        // Need this structure for Merkle proof verification
        let mut bowp = BlockOutputWithProofs {
            logs: vec![],
            leaf_count: 0,
        };

        for log in job.clone().into_merkle_paths() {
            //tracing::info!("log.merkle_paths.len()={:?}", log.merkle_paths.len());
            // log.value_read and log.value_written can both be non-null, if the value is updated
            //assert!(!(log.value_read != [0; 32] && log.value_written != [0; 32]));
            //tracing::info!("{:#?}", log);
            if !log.is_write {
                for (k, v) in wbs.read_storage_key.iter() {
                    if k.hashed_key_u256() == log.leaf_hashed_key {
                        let hasher = Blake2Hasher {};
                        hasher.hash_bytes(&v.0);
                        assert_eq!(zksync_types::H256(log.value_read), *v);
                        //tracing::info!("read, key={:?}, value={:?}", k, v);
                        //tracing::info!("{:#?}", log);
                    }
                }
            }

            let e = {
                if !log.is_write && log.leaf_enumeration_index == 0 {
                    TreeLogEntry::ReadMissingKey
                } else if !log.is_write && log.leaf_enumeration_index != 0 {
                    TreeLogEntry::Read {
                        leaf_index: log.leaf_enumeration_index,
                        value: zksync_types::H256(log.value_read),
                    }
                } else if log.is_write && log.first_write {
                    TreeLogEntry::Inserted
                } else if log.is_write && !log.first_write {
                    TreeLogEntry::Updated {
                        leaf_index: log.leaf_enumeration_index,
                        previous_value: zksync_types::H256(log.value_read),
                    }
                } else {
                    panic!();
                }
            };

            // let e = match (log.is_write, log.leaf_enumeration_index) {
            //     (false, 0) => TreeLogEntry::ReadMissingKey,
            //     (false, _) => TreeLogEntry::Read { leaf_index: log.leaf_enumeration_index, value: log.value_read },
            //     (true, )
            // }
            // let e = TreeLogEntry::Read { leaf_index: (), value: () };
            // let e: TreeLogEntry::ReadMissingKey;
            // let e: TreeLogEntry::Write();
            // let e: TreeLogEntry::Updated
            let tle = TreeLogEntryWithProof {
                base: e,
                merkle_path: log
                    .merkle_paths
                    .iter()
                    .map(|x| zksync_types::H256(*x))
                    .collect(),
                root_hash: zksync_types::H256(log.root_hash),
            };
            bowp.logs.push(tle);
        }
        // BlockOutputWithProofs -> this is what we need to reconstruct, to verify Merkle proofs
        tracing::info!("bowp.len()={:?}", bowp.logs.len());

        //let tlewp: TreeLogEntryWithProof {base: TreeLogEntry {}};

        let old_root_hash = rt_handle
            .block_on(
                connection
                    .blocks_dal()
                    .get_l1_batch_state_root(l1_batch_number - 1),
            )
            .unwrap()
            .unwrap();
        tracing::info!("{:?}", old_root_hash);

        let (mut vm, storage_view) =
            create_vm(rt_handle.clone(), l1_batch_number, connection, l2_chain_id)
                .context("failed to create vm for Sgx2fa")?;

        tracing::info!("Started execution of l1_batch: {l1_batch_number:?}");

        let next_miniblocks_data = miniblocks_execution_data
            .iter()
            .skip(1)
            .map(Some)
            .chain([None]);
        let miniblocks_data = miniblocks_execution_data.iter().zip(next_miniblocks_data);

        for (miniblock_data, next_miniblock_data) in miniblocks_data {
            tracing::debug!(
                "Started execution of miniblock: {:?}, executing {:?} transactions",
                miniblock_data.number,
                miniblock_data.txs.len(),
            );
            for tx in &miniblock_data.txs {
                tracing::trace!("Started execution of tx: {tx:?}");
                execute_tx(tx, &mut vm).context("failed to execute transaction in Sgx2fa")?;
                tracing::trace!("Finished execution of tx: {tx:?}");
            }
            if let Some(next_miniblock_data) = next_miniblock_data {
                vm.start_new_l2_block(L2BlockEnv::from_miniblock_data(next_miniblock_data));
            }

            tracing::debug!(
                "Finished execution of miniblock: {:?}",
                miniblock_data.number
            );
        }

        let vm_out = vm.finish_batch();
        tracing::info!("=== FinishedL1Batch ===");
        //tracing::info!("{:#X?}", vm_out);
        tracing::info!("+++ FinishedL1Batch +++");
        tracing::info!(
            "vm_out.final_execution_state.storage_log_queries.len()={:?}",
            vm_out.final_execution_state.storage_log_queries.len()
        );
        tracing::info!(
            "vm_out.final_execution_state.deduplicated_storage_log_queries.len()={:?}",
            vm_out
                .final_execution_state
                .deduplicated_storage_log_queries
                .len()
        );
        tracing::info!(
            "vm_out.block_tip_execution_result.logs.storage_logs.len()={:?}",
            vm_out.block_tip_execution_result.logs.storage_logs.len()
        );
        tracing::info!("Finished execution of l1_batch: {l1_batch_number:?}");

        let mut instructions = Vec::<TreeInstruction>::new();

        let mut idx = job.next_enumeration_index();

        for (a, b) in vm_out
            .final_execution_state
            .deduplicated_storage_log_queries
            .iter()
            .zip(bowp.logs.iter())
        {
            // It is either a read or a write but never both?
            // Hmhm, how do we map LogQuery to TreeInstruction if both read_value and written_value are set?
            if a.read_value != U256([0u64; 4]) && a.written_value != U256([0u64; 4]) {
                // assert!(a.rw_flag && (a.read_value != a.written_value));
                // assert!(!a.rw_flag && (a.read_value == a.written_value));

                tracing::info!("{:?}", a);
                tracing::info!("{:?}", b.base);
            }

            let key = StorageKey::new(
                AccountTreeId::new(a.address),
                zksync_types::H256(a.key.into()),
            )
            .hashed_key_u256();

            if a.rw_flag {
                let instruction = match b.base {
                    TreeLogEntry::Updated {
                        leaf_index,
                        previous_value,
                    } => TreeInstruction::write(
                        key,
                        leaf_index,
                        zksync_types::H256(a.written_value.into()),
                    ),
                    TreeLogEntry::Inserted => {
                        let ret = TreeInstruction::write(
                            key,
                            idx,
                            zksync_types::H256(a.written_value.into()),
                        );
                        idx += 1;
                        ret
                    }
                    _ => panic!(),
                };
                instructions.push(instruction);
            } else {
                match b.base {
                    TreeLogEntry::Read { leaf_index, value } => (),
                    TreeLogEntry::ReadMissingKey => (),
                    _ => assert!(false),
                }
                instructions.push(TreeInstruction::Read(key));
            }

            tracing::info!("{:?}", a);
            tracing::info!("{:?}", b.base);

            // if let TreeLogEntry::Inserted = b.base {
            //     assert!(a.written_value != U256([0; 32]));
            // }
            if let TreeLogEntry::Read { leaf_index, value } = b.base {
                assert_eq!(a.read_value, value.into_uint());
                tracing::info!("Whoop, whoop!");
            }
        }

        // `verify_proofs` will panic!() if something does not add up.
        bowp.verify_proofs(&Blake2Hasher, old_root_hash, &instructions);

        tracing::info!("Looks like we verified {l1_batch_number} correctly - whoop, whoop!");

        METRICS.process_batch_time.observe(started_at.elapsed());
        tracing::info!(
            "Sgx2fa took {:?} for L1BatchNumber {}",
            started_at.elapsed(),
            l1_batch_number.0
        );

        let witness_block_state = (*storage_view).borrow().witness_block_state();
        Ok(witness_block_state)
    }
}

#[async_trait]
impl JobProcessor for Sgx2fa {
    type Job = L1BatchNumber;
    type JobId = L1BatchNumber;
    type JobArtifacts = WitnessBlockState;
    const SERVICE_NAME: &'static str = "sgx_2fa";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut connection = self.connection_pool.access_storage().await?;
        let l1_batch_to_process = connection
            .sgx_2fa_dal()
            .get_next_sgx_2fa_job()
            .await
            .context("failed to get next basic witness input producer job")?;
        Ok(l1_batch_to_process.map(|number| (number, number)))
    }

    async fn save_failure(&self, job_id: Self::JobId, started_at: Instant, error: String) {
        let attempts = self
            .connection_pool
            .access_storage()
            .await
            .unwrap()
            .basic_witness_input_producer_dal()
            .mark_job_as_failed(job_id, started_at, error)
            .await
            .expect("errored whilst marking job as failed");
        if let Some(tries) = attempts {
            tracing::warn!("Failed to process job: {job_id:?}, after {tries} tries.");
        } else {
            tracing::warn!("L1 Batch {job_id:?} was processed successfully by another worker.");
        }
    }

    async fn process_job(
        &self,
        job: Self::Job,
        started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        let l2_chain_id = self.l2_chain_id;
        let connection_pool = self.connection_pool.clone();
        let object_store = self.object_store.clone();
        tokio::task::spawn_blocking(move || {
            let rt_handle = Handle::current();
            Self::process_job_impl(
                rt_handle,
                job,
                started_at,
                connection_pool.clone(),
                l2_chain_id,
                object_store,
            )
        })
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
    ) -> anyhow::Result<()> {
        let upload_started_at = Instant::now();
        let object_path = self
            .object_store
            .put(job_id, &artifacts)
            .await
            .context("failed to upload artifacts for BasicWitnessInputProducer")?;
        METRICS
            .upload_input_time
            .observe(upload_started_at.elapsed());
        let mut connection = self
            .connection_pool
            .access_storage()
            .await
            .context("failed to acquire DB connection for BasicWitnessInputProducer")?;
        let mut transaction = connection
            .start_transaction()
            .await
            .context("failed to acquire DB transaction for BasicWitnessInputProducer")?;
        transaction
            .basic_witness_input_producer_dal()
            .mark_job_as_successful(job_id, started_at, &object_path)
            .await
            .context("failed to mark job as successful for BasicWitnessInputProducer")?;
        transaction
            .commit()
            .await
            .context("failed to commit DB transaction for BasicWitnessInputProducer")?;
        METRICS.block_number_processed.set(job_id.0 as i64);
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        JOB_MAX_ATTEMPT as u32
    }

    async fn get_job_attempts(&self, job_id: &L1BatchNumber) -> anyhow::Result<u32> {
        let mut connection = self
            .connection_pool
            .access_storage()
            .await
            .context("failed to acquire DB connection for BasicWitnessInputProducer")?;
        connection
            .basic_witness_input_producer_dal()
            .get_basic_witness_input_producer_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for BasicWitnessInputProducer")
    }
}
