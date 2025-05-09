use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use proof_compression_gpu::{run_proof_chain, SnarkWrapper, SnarkWrapperProof};
use tokio::task::JoinHandle;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        circuit_definitions::recursion_layer::ZkSyncRecursionLayerProof,
        zkevm_circuits::scheduler::block_header::BlockAuxilaryOutputWitness,
    },
    get_current_pod_name, AuxOutputWitnessWrapper, FriProofWrapper,
};
use zksync_prover_interface::{
    outputs::{
        FflonkL1BatchProofForL1, L1BatchProofForL1, L1BatchProofForL1Key, PlonkL1BatchProofForL1,
    },
    CBOR,
};
use zksync_prover_keystore::keystore::Keystore;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{protocol_version::ProtocolSemanticVersion, L1BatchId};

use crate::metrics::METRICS;

pub struct ProofCompressor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Prover>,
    max_attempts: u32,
    protocol_version: ProtocolSemanticVersion,
    keystore: Keystore,
    is_fflonk: bool,
}

impl ProofCompressor {
    pub fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Prover>,
        max_attempts: u32,
        protocol_version: ProtocolSemanticVersion,
        keystore: Keystore,
        is_fflonk: bool,
    ) -> Self {
        Self {
            blob_store,
            pool,
            max_attempts,
            protocol_version,
            keystore,
            is_fflonk,
        }
    }

    fn aux_output_witness_to_array(
        aux_output_witness: BlockAuxilaryOutputWitness<GoldilocksField>,
    ) -> [[u8; 32]; 4] {
        let mut array: [[u8; 32]; 4] = [[0; 32]; 4];

        for i in 0..32 {
            array[0][i] = aux_output_witness.l1_messages_linear_hash[i];
            array[1][i] = aux_output_witness.rollup_state_diff_for_compression[i];
            array[2][i] = aux_output_witness.bootloader_heap_initial_content[i];
            array[3][i] = aux_output_witness.events_queue_state[i];
        }
        array
    }
}

#[async_trait]
impl JobProcessor for ProofCompressor {
    type Job = ZkSyncRecursionLayerProof;
    type JobId = L1BatchId;

    type JobArtifacts = SnarkWrapperProof;

    const SERVICE_NAME: &'static str = "ProofCompressor";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut conn = self.pool.connection().await.unwrap();
        let pod_name = get_current_pod_name();
        let Some(l1_batch_id) = conn
            .fri_proof_compressor_dal()
            .get_next_proof_compression_job(&pod_name, self.protocol_version)
            .await
        else {
            return Ok(None);
        };
        let Some(fri_proof_id) = conn
            .fri_prover_jobs_dal()
            .get_scheduler_proof_job_id(l1_batch_id)
            .await
        else {
            anyhow::bail!("Scheduler proof is missing from database for batch {l1_batch_id}");
        };
        tracing::info!("Started proof compression for L1 batch: {l1_batch_id}");
        let observer = METRICS.blob_fetch_time.start();

        let fri_proof: FriProofWrapper = self.blob_store.get((fri_proof_id, l1_batch_id.chain_id()))
            .await.with_context(|| format!("Failed to get fri proof from blob store for {l1_batch_id} with id {fri_proof_id}"))?;

        observer.observe();

        let scheduler_proof = match fri_proof {
            FriProofWrapper::Base(_) => anyhow::bail!("Must be a scheduler proof not base layer"),
            FriProofWrapper::Recursive(proof) => proof,
        };
        Ok(Some((l1_batch_id, scheduler_proof)))
    }

    async fn save_failure(&self, job_id: Self::JobId, _started_at: Instant, error: String) {
        self.pool
            .connection()
            .await
            .expect("failed to acquire DB connection for ProofCompressor")
            .fri_proof_compressor_dal()
            .mark_proof_compression_job_failed(&error, job_id)
            .await
            .expect("failed to mark proof compression job failed");
    }

    async fn process_job(
        &self,
        _job_id: &L1BatchId,
        job: ZkSyncRecursionLayerProof,
        _started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        let keystore = self.keystore.clone();
        let snark_wrapper_mode = if self.is_fflonk {
            SnarkWrapper::FFfonk
        } else {
            SnarkWrapper::Plonk
        };

        tokio::task::spawn_blocking(move || {
            Ok(run_proof_chain(
                snark_wrapper_mode,
                &keystore,
                job.into_inner(),
            ))
        })
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
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
        let aggregation_result_coords =
            Self::aux_output_witness_to_array(aux_output_witness_wrapper.0);

        let l1_batch_proof: L1BatchProofForL1<CBOR> = match artifacts {
            SnarkWrapperProof::Plonk(proof) => {
                L1BatchProofForL1::new_plonk(PlonkL1BatchProofForL1 {
                    aggregation_result_coords,
                    scheduler_proof: proof,
                    protocol_version: self.protocol_version,
                })
            }
            SnarkWrapperProof::FFfonk(proof) => {
                L1BatchProofForL1::new_fflonk(FflonkL1BatchProofForL1 {
                    aggregation_result_coords,
                    scheduler_proof: proof,
                    protocol_version: self.protocol_version,
                })
            }
        };

        let blob_save_started_at = Instant::now();
        let blob_url = self
            .blob_store
            .put(
                L1BatchProofForL1Key::Prover((job_id, self.protocol_version)),
                &l1_batch_proof,
            )
            .await
            .context("Failed to save converted l1_batch_proof")?;
        METRICS
            .blob_save_time
            .observe(blob_save_started_at.elapsed());

        self.pool
            .connection()
            .await
            .expect("failed to acquire DB connection for ProofCompressor")
            .fri_proof_compressor_dal()
            .mark_proof_compression_job_successful(job_id, started_at.elapsed(), &blob_url)
            .await
            .expect("failed to mark proof compression job successful");
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    async fn get_job_attempts(&self, job_id: &L1BatchId) -> anyhow::Result<u32> {
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
}
