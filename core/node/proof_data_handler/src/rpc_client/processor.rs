use std::sync::Arc;

use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::{
    api::{ProofGenerationData, SubmitProofRequest},
    inputs::{
        L1BatchMetadataHashes, VMRunWitnessInputData, WitnessInputData, WitnessInputMerklePaths,
    },
};
use zksync_types::{
    basic_fri_types::Eip4844Blobs,
    commitment::{serialize_commitments, L1BatchCommitmentMode},
    web3::keccak256,
    L1BatchNumber, ProtocolVersionId, H256, STATE_DIFF_HASH_KEY_PRE_GATEWAY,
};

use crate::metrics::METRICS;

#[derive(Debug)]
pub struct ProofDataProcessor {
    pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    commitment_mode: L1BatchCommitmentMode,
}

impl ProofDataProcessor {
    pub fn new(
        pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            pool,
            blob_store,
            commitment_mode,
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) async fn get_proof_generation_data(
        &self,
    ) -> anyhow::Result<Option<ProofGenerationData>> {
        let l1_batch_number = match self.lock_batch_for_proving().await? {
            Some(number) => number,
            None => return Ok(None), // no batches pending to be proven
        };

        Ok(Some(
            self.proof_generation_data_for_existing_batch(l1_batch_number)
                .await?,
        ))
    }

    /// Will choose a batch that has all the required data and isn't picked up by any prover yet.
    pub(crate) async fn lock_batch_for_proving(&self) -> anyhow::Result<Option<L1BatchNumber>> {
        self.pool
            .connection()
            .await?
            .proof_generation_dal()
            .lock_batch_for_proving()
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub(crate) async fn unlock_batch(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<()> {
        self.pool
            .connection()
            .await?
            .proof_generation_dal()
            .unlock_batch(l1_batch_number)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Will fetch all the required data for the batch and return it.
    ///
    /// ## Panics
    ///
    /// Expects all the data to be present in the database.
    /// Will panic if any of the required data is missing.
    #[tracing::instrument(skip(self))]
    async fn proof_generation_data_for_existing_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<ProofGenerationData> {
        let vm_run_data: VMRunWitnessInputData = self.blob_store.get(l1_batch_number).await?;
        let merkle_paths: WitnessInputMerklePaths = self.blob_store.get(l1_batch_number).await?;

        // Acquire connection after interacting with GCP, to avoid holding the connection for too long.
        let mut conn = self.pool.connection().await?;

        let previous_batch_metadata = conn
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(l1_batch_number.checked_sub(1).unwrap()))
            .await?
            .expect("No metadata for previous batch");

        let header = conn
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?
            .unwrap_or_else(|| panic!("Missing header for {}", l1_batch_number));

        let minor_version = header.protocol_version.unwrap();
        let protocol_version = conn
            .protocol_versions_dal()
            .get_protocol_version_with_latest_patch(minor_version)
            .await?
            .unwrap_or_else(|| {
                panic!("Missing l1 verifier info for protocol version {minor_version}")
            });

        let batch_header = conn
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?
            .unwrap_or_else(|| panic!("Missing header for {}", l1_batch_number));

        let eip_4844_blobs = match self.commitment_mode {
            L1BatchCommitmentMode::Validium => Eip4844Blobs::empty(),
            L1BatchCommitmentMode::Rollup => {
                let blobs = batch_header.pubdata_input.as_deref().unwrap_or_else(|| {
                    panic!(
                        "expected pubdata, but it is not available for batch {l1_batch_number:?}"
                    )
                });
                Eip4844Blobs::decode(blobs).expect("failed to decode EIP-4844 blobs")
            }
        };

        let blob = WitnessInputData {
            vm_run_data,
            merkle_paths,
            eip_4844_blobs,
            previous_batch_metadata: L1BatchMetadataHashes {
                root_hash: previous_batch_metadata.metadata.root_hash,
                meta_hash: previous_batch_metadata.metadata.meta_parameters_hash,
                aux_hash: previous_batch_metadata.metadata.aux_data_hash,
            },
        };

        METRICS.observe_blob_sizes(&blob);

        Ok(ProofGenerationData {
            l1_batch_number,
            witness_input_data: blob,
            protocol_version: protocol_version.version,
            l1_verifier_config: protocol_version.l1_verifier_config,
        })
    }

    pub(crate) async fn handle_proof(&self, proof: SubmitProofRequest) -> anyhow::Result<()> {
        match proof {
            SubmitProofRequest::Proof(l1_batch_number, proof) => {
                tracing::info!("Received proof for block number: {:?}", l1_batch_number);

                let blob_url = self
                    .blob_store
                    .put((l1_batch_number, proof.protocol_version()), &*proof)
                    .await?;

                let aggregation_coords = proof.aggregation_result_coords();

                let system_logs_hash_from_prover = H256::from_slice(&aggregation_coords[0]);
                let state_diff_hash_from_prover = H256::from_slice(&aggregation_coords[1]);
                let bootloader_heap_initial_content_from_prover =
                    H256::from_slice(&aggregation_coords[2]);
                let events_queue_state_from_prover = H256::from_slice(&aggregation_coords[3]);

                let mut storage = self.pool.connection().await.unwrap();

                let l1_batch = storage
                    .blocks_dal()
                    .get_l1_batch_metadata(l1_batch_number)
                    .await
                    .unwrap()
                    .expect("Proved block without metadata");

                let protocol_version = l1_batch
                    .header
                    .protocol_version
                    .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);

                let events_queue_state = l1_batch
                    .metadata
                    .events_queue_commitment
                    .expect("No events_queue_commitment");
                let bootloader_heap_initial_content = l1_batch
                    .metadata
                    .bootloader_initial_content_commitment
                    .expect("No bootloader_initial_content_commitment");

                if events_queue_state != events_queue_state_from_prover
                    || bootloader_heap_initial_content
                        != bootloader_heap_initial_content_from_prover
                {
                    panic!(
                        "Auxilary output doesn't match\n\
                        server values: events_queue_state = {events_queue_state}, bootloader_heap_initial_content = {bootloader_heap_initial_content}\n\
                        prover values: events_queue_state = {events_queue_state_from_prover}, bootloader_heap_initial_content = {bootloader_heap_initial_content_from_prover}",
                    );
                }

                let system_logs = serialize_commitments(&l1_batch.header.system_logs);
                let system_logs_hash = H256(keccak256(&system_logs));

                let state_diff_hash = if protocol_version.is_pre_gateway() {
                    l1_batch
                        .header
                        .system_logs
                        .iter()
                        .find_map(|log| {
                            (log.0.key
                                == H256::from_low_u64_be(STATE_DIFF_HASH_KEY_PRE_GATEWAY as u64))
                            .then_some(log.0.value)
                        })
                        .expect("Failed to get state_diff_hash from system logs")
                } else {
                    l1_batch
                        .metadata
                        .state_diff_hash
                        .expect("Failed to get state_diff_hash from metadata")
                };

                if state_diff_hash != state_diff_hash_from_prover
                    || system_logs_hash != system_logs_hash_from_prover
                {
                    let server_values = format!("system_logs_hash = {system_logs_hash}, state_diff_hash = {state_diff_hash}");
                    let prover_values = format!("system_logs_hash = {system_logs_hash_from_prover}, state_diff_hash = {state_diff_hash_from_prover}");
                    panic!(
                        "Auxilary output doesn't match, server values: {} prover values: {}",
                        server_values, prover_values
                    );
                }

                storage
                    .proof_generation_dal()
                    .save_proof_artifacts_metadata(l1_batch_number, &blob_url)
                    .await?;
            }
            SubmitProofRequest::SkippedProofGeneration(l1_batch_number) => {
                tracing::info!("Skipped proof for block number: {:?}", l1_batch_number);
                self.pool
                    .connection()
                    .await
                    .unwrap()
                    .proof_generation_dal()
                    .mark_proof_generation_job_as_skipped(l1_batch_number)
                    .await?;
            }
        }

        Ok(())
    }
}
