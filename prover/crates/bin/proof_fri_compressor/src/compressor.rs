use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use async_trait::async_trait;
use circuit_sequencer_api::proof::FinalProof;
use fflonk_gpu::{FflonkSnarkVerifierCircuit, FflonkSnarkVerifierCircuitProof};
use tokio::{sync::watch, task::JoinHandle};
use vise::{Buckets, Counter, Histogram, LabeledFamily, Metrics};
use wrapper_prover::{GPUWrapperConfigs, WrapperProver};
use zkevm_test_harness::proof_wrapper_utils::{get_trusted_setup, DEFAULT_WRAPPER_CONFIG};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::circuit_definitions::recursion_layer::{
        ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType,
    },
    get_current_pod_name, AuxOutputWitnessWrapper, FriProofWrapper,
};
use zksync_prover_interface::outputs::{
    FflonkL1BatchProofForL1, L1BatchProofForL1, PlonkL1BatchProofForL1,
};
use zksync_prover_keystore::keystore::Keystore;
use zksync_types::{protocol_version::ProtocolSemanticVersion, L1BatchNumber};
use zksync_utils::panic_extractor::try_extract_panic_message;

use crate::{
    fflonk,
    fflonk::generate_fflonk_proof,
    metrics::METRICS,
    plonk::generate_plonk_proof,
    traits::{Compressor, CrsLoader, SnarkProver, SnarkSetupDataLoader},
    utils::aux_output_witness_to_array,
};

pub struct ProofCompressor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Prover>,
    compression_mode: u8,
    max_attempts: u32,
    protocol_version: ProtocolSemanticVersion,
    keystore: Keystore,
    is_fflonk: bool,
}

pub enum Proof {
    Plonk(Box<FinalProof>),
    Fflonk(FflonkSnarkVerifierCircuitProof),
}

impl ProofCompressor {
    const SERVICE_NAME: &'static str = "ProofCompressor";
    const POLLING_INTERVAL_MS: u64 = 1000;
    const MAX_BACKOFF_MS: u64 = 60_000;
    const BACKOFF_MULTIPLIER: u64 = 2;

    pub fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Prover>,
        compression_mode: u8,
        max_attempts: u32,
        protocol_version: ProtocolSemanticVersion,
        keystore: Keystore,
        is_fflonk: bool,
    ) -> Self {
        Self {
            blob_store,
            pool,
            compression_mode,
            max_attempts,
            protocol_version,
            keystore,
            is_fflonk,
        }
    }

    async fn get_next_job(
        &self,
    ) -> anyhow::Result<Option<(L1BatchNumber, ZkSyncRecursionLayerProof)>> {
        let mut conn = self.pool.connection().await.unwrap();
        let pod_name = get_current_pod_name();
        let Some(l1_batch_number) = conn
            .fri_proof_compressor_dal()
            .get_next_proof_compression_job(&pod_name, self.protocol_version)
            .await
        else {
            return Ok(None);
        };
        let Some(fri_proof_id) = conn
            .fri_prover_jobs_dal()
            .get_scheduler_proof_job_id(l1_batch_number)
            .await
        else {
            anyhow::bail!("Scheduler proof is missing from database for batch {l1_batch_number}");
        };
        tracing::info!(
            "Started proof compression for L1 batch: {:?}",
            l1_batch_number
        );
        let observer = METRICS.blob_fetch_time.start();

        let fri_proof: FriProofWrapper = self.blob_store.get(fri_proof_id)
            .await.with_context(|| format!("Failed to get fri proof from blob store for {l1_batch_number} with id {fri_proof_id}"))?;

        observer.observe();

        let scheduler_proof = match fri_proof {
            FriProofWrapper::Base(_) => anyhow::bail!("Must be a scheduler proof not base layer"),
            FriProofWrapper::Recursive(proof) => proof,
        };
        Ok(Some((l1_batch_number, scheduler_proof)))
    }

    async fn save_failure(&self, job_id: L1BatchNumber, _started_at: Instant, error: String) {
        self.pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .mark_proof_compression_job_failed(&error, job_id)
            .await;
    }

    fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    async fn get_job_attempts(&self, job_id: &L1BatchNumber) -> anyhow::Result<u32> {
        let mut prover_storage = self
            .pool
            .connection()
            .await
            .context("failed to acquire DB connection for ProofCompressor")?;
        prover_storage
            .fri_proof_compressor_dal()
            .get_proof_compression_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for ProofCompressor")
    }

    pub async fn run(
        self,
        mut stop_receiver: watch::Receiver<bool>,
        mut iterations_left: Option<usize>,
    ) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let mut backoff: u64 = Self::POLLING_INTERVAL_MS;
        while iterations_left.map_or(true, |i| i > 0) {
            if *stop_receiver.borrow() {
                tracing::warn!(
                    "Stop signal received, shutting down {} component while waiting for a new job",
                    Self::SERVICE_NAME
                );
                return Ok(());
            }
            if let Some((job_id, job)) =
                Self::get_next_job(&self).await.context("get_next_job()")?
            {
                let started_at = Instant::now();
                backoff = Self::POLLING_INTERVAL_MS;
                iterations_left = iterations_left.map(|i| i - 1);

                tracing::debug!(
                    "Spawning thread processing {:?} job with id {:?}",
                    Self::SERVICE_NAME,
                    job_id
                );

                if self.is_fflonk {
                    let started_at = Instant::now();
                    let task = self.generate_fflonk_proof(job).await?;
                    let proof = Proof::Fflonk(task);
                    tracing::info!(
                        "Compression&Proof generation took: {:?}",
                        started_at.elapsed()
                    );

                    self.save_result(job_id, started_at, proof).await?;
                }
                // else{
                //      let task = tokio::task::spawn_blocking(async ||{generate_plonk_proof(job, self.keystore.clone()).await});
                //
                //      self.wait_for_task(job_id, started_at, task, &mut stop_receiver)
                //          .await
                //          .context("wait_for_task")?;
                // }
            } else if iterations_left.is_some() {
                tracing::info!("No more jobs to process. Server can stop now.");
                return Ok(());
            } else {
                tracing::trace!("Backing off for {} ms", backoff);
                // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
                tokio::time::timeout(Duration::from_millis(backoff), stop_receiver.changed())
                    .await
                    .ok();
                backoff = (backoff * Self::BACKOFF_MULTIPLIER).min(Self::MAX_BACKOFF_MS);
            }
        }
        tracing::info!("Requested number of jobs is processed. Server can stop now.");
        Ok(())
    }

    async fn save_result(
        &self,
        job_id: L1BatchNumber,
        started_at: Instant,
        artifacts: Proof,
    ) -> anyhow::Result<()> {
        METRICS.compression_time.observe(started_at.elapsed());
        tracing::info!(
            "Finished fri proof compression for job: {job_id} took: {:?}",
            started_at.elapsed()
        );

        let aux_output_witness_wrapper: AuxOutputWitnessWrapper = self
            .blob_store
            .get(job_id)
            .await
            .context("Failed to get aggregation result coords from blob store")?;
        let aggregation_result_coords = aux_output_witness_to_array(aux_output_witness_wrapper.0);

        let l1_batch_proof = match artifacts {
            Proof::Plonk(proof) => L1BatchProofForL1::Plonk(PlonkL1BatchProofForL1 {
                aggregation_result_coords,
                scheduler_proof: *proof,
                protocol_version: self.protocol_version,
            }),
            Proof::Fflonk(proof) => L1BatchProofForL1::Fflonk(FflonkL1BatchProofForL1 {
                aggregation_result_coords,
                scheduler_proof: proof,
                protocol_version: self.protocol_version,
            }),
        };

        let blob_save_started_at = Instant::now();
        let blob_url = self
            .blob_store
            .put((job_id, self.protocol_version), &l1_batch_proof)
            .await
            .context("Failed to save converted l1_batch_proof")?;
        METRICS
            .blob_save_time
            .observe(blob_save_started_at.elapsed());

        self.pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .mark_proof_compression_job_successful(job_id, started_at.elapsed(), &blob_url)
            .await;
        Ok(())
    }

    async fn wait_for_task(
        &self,
        job_id: L1BatchNumber,
        started_at: Instant,
        task: JoinHandle<anyhow::Result<Proof>>,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let attempts = self.get_job_attempts(&job_id).await?;
        let max_attempts = self.max_attempts();
        if attempts == max_attempts {
            METRICS.max_attempts_reached[&(Self::SERVICE_NAME, format!("{job_id:?}"))].inc();
            tracing::error!(
                "Max attempts ({max_attempts}) reached for {} job {:?}",
                Self::SERVICE_NAME,
                job_id,
            );
        }

        let result = loop {
            tracing::trace!(
                "Polling {} task with id {:?}. Is finished: {}",
                Self::SERVICE_NAME,
                job_id,
                task.is_finished()
            );
            if task.is_finished() {
                break task.await;
            }
            if tokio::time::timeout(
                Duration::from_millis(Self::POLLING_INTERVAL_MS),
                stop_receiver.changed(),
            )
            .await
            .is_ok()
            {
                // Stop signal received, return early.
                // Exit will be processed/reported by the main loop.
                return Ok(());
            }
        };
        let error_message = match result {
            Ok(Ok(data)) => {
                tracing::debug!(
                    "{} Job {:?} finished successfully",
                    Self::SERVICE_NAME,
                    job_id
                );
                METRICS.attempts[&Self::SERVICE_NAME].observe(attempts as usize);
                return self
                    .save_result(job_id, started_at, data)
                    .await
                    .context("save_result()");
            }
            Ok(Err(error)) => error.to_string(),
            Err(error) => try_extract_panic_message(error),
        };
        tracing::error!(
            "Error occurred while processing {} job {:?}: {:?}",
            Self::SERVICE_NAME,
            job_id,
            error_message
        );

        self.save_failure(job_id, started_at, error_message).await;
        Ok(())
    }

    async fn generate_fflonk_proof(
        &self,
        job: ZkSyncRecursionLayerProof,
    ) -> anyhow::Result<FflonkSnarkVerifierCircuitProof> {
        let (setup_data_sender, setup_data_receiver) = tokio::sync::mpsc::channel(1);
        let (crs_sender, crs_receiver) = tokio::sync::mpsc::channel(1);
        let (compression_data_sender, compression_data_receiver) = tokio::sync::mpsc::channel(1);

        let setup_data_loader = SnarkSetupDataLoader::new(self.keystore.clone(), setup_data_sender);
        let crs_loader = CrsLoader::new(String::new(), crs_sender);

        let scheduler_vk = self
            .keystore
            .load_recursive_layer_verification_key(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            )
            .context("get_recursiver_layer_vk_for_circuit_type()")?;

        let compressor = Compressor::new(
            self.keystore.clone(),
            self.compression_mode,
            job,
            scheduler_vk,
            compression_data_sender,
        );
        let mut snark_prover = SnarkProver::new(
            setup_data_receiver,
            crs_receiver,
            compression_data_receiver,
            self.compression_mode,
        );

        let setup_data_task = setup_data_loader.run();
        let crs_data_task = crs_loader.run();
        let compressor_task = compressor.run();
        let snark_prover_task = snark_prover.run();

        let (_, _, _, proof) = tokio::join!(
            tokio::spawn(setup_data_task),
            tokio::spawn(crs_data_task),
            tokio::spawn(compressor_task),
            tokio::spawn(snark_prover_task)
        );
        proof?
    }
}
