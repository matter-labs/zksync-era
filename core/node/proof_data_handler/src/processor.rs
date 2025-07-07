use std::{sync::Arc, time::Duration};

use zksync_config::configs::proof_data_handler::ProvingMode;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::{ObjectStore, StoredObject};
use zksync_prover_interface::{
    api::ProofGenerationData,
    inputs::{
        L1BatchMetadataHashes, VMRunWitnessInputData, WitnessInputData, WitnessInputMerklePaths,
    },
    outputs::{L1BatchProofForL1, L1BatchProofForL1Key},
    Bincode, CBOR,
};
use zksync_types::{
    basic_fri_types::Eip4844Blobs,
    commitment::{serialize_commitments, L1BatchCommitmentMode},
    protocol_version::ProtocolSemanticVersion,
    web3::keccak256,
    L1BatchId, L1BatchNumber, L2ChainId, ProtocolVersionId, H256, STATE_DIFF_HASH_KEY_PRE_GATEWAY,
};

use crate::{errors::ProcessorError, metrics::METRICS};

pub trait ProcessorMode {}

/// Use this mode if you want to use the processor for testing.
/// It will not lock the batch for proving and perform all the operations in read-only mode.
#[derive(Clone)]
pub struct Readonly;

/// Use this mode if you want to use the processor for actual proving.
/// It will lock the batch for proving and unlock it if the proof generation fails.
#[derive(Clone)]
pub struct Locking;

impl ProcessorMode for Readonly {}
impl ProcessorMode for Locking {}

#[derive(Clone)]
pub struct Processor<PM: ProcessorMode> {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    proof_generation_timeout: Duration,
    chain_id: L2ChainId,
    proving_mode: ProvingMode,
    _marker: std::marker::PhantomData<PM>,
}

impl<PM: ProcessorMode> Processor<PM> {
    pub fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Core>,
        proof_generation_timeout: Duration,
        chain_id: L2ChainId,
        proving_mode: ProvingMode,
    ) -> Self {
        Self {
            blob_store,
            pool,
            proof_generation_timeout,
            chain_id,
            proving_mode,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn chain_id(&self) -> L2ChainId {
        self.chain_id
    }

    pub async fn get_oldest_not_proven_batch(
        &self,
    ) -> Result<Option<L1BatchNumber>, ProcessorError> {
        self.pool
            .connection()
            .await
            .unwrap()
            .proof_generation_dal()
            .get_oldest_not_generated_batch()
            .await
            .map_err(Into::into)
    }

    /// Will fetch all the required data for the batch and return it.
    ///
    /// ## Panics
    ///
    /// Expects all the data to be present in the database.
    /// Will panic if any of the required data is missing.
    #[tracing::instrument(skip(self))]
    pub async fn proof_generation_data_for_existing_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<ProofGenerationData, ProcessorError> {
        let vm_run_data: VMRunWitnessInputData = match self.blob_store.get(l1_batch_number).await {
            Ok(data) => data,
            Err(_) => self
                .blob_store
                .get::<VMRunWitnessInputData<Bincode>>(l1_batch_number)
                .await
                .map(Into::into)?,
        };

        let merkle_paths: WitnessInputMerklePaths = match self.blob_store.get(l1_batch_number).await
        {
            Ok(data) => data,
            Err(_) => self
                .blob_store
                .get::<WitnessInputMerklePaths<Bincode>>(l1_batch_number)
                .await
                .map(Into::into)?,
        };

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

        let pubdata_params = conn
            .blocks_dal()
            .get_l1_batch_pubdata_params(l1_batch_number)
            .await?
            .unwrap_or_else(|| panic!("Missing pubdata params for {l1_batch_number}"));

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

        let eip_4844_blobs = match pubdata_params.pubdata_type.into() {
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

        let batch_sealed_at = conn
            .blocks_dal()
            .get_batch_sealed_at(l1_batch_number)
            .await?
            .ok_or(ProcessorError::GeneralError(format!(
                "Batch {l1_batch_number} not found in blocks_dal"
            )))?;

        Ok(ProofGenerationData {
            l1_batch_number,
            chain_id: self.chain_id,
            batch_sealed_at,
            witness_input_data: blob,
            protocol_version: protocol_version.version,
            l1_verifier_config: protocol_version.l1_verifier_config,
        })
    }
}

impl Processor<Locking> {
    #[tracing::instrument(skip_all)]
    pub(crate) async fn get_proof_generation_data(
        &self,
    ) -> Result<Option<ProofGenerationData>, ProcessorError> {
        let l1_batch_number = match self
            .lock_batch_for_proving(self.proof_generation_timeout)
            .await?
        {
            Some(number) => number,
            None => return Ok(None), // no batches pending to be proven
        };

        let proof_generation_data = self
            .proof_generation_data_for_existing_batch(l1_batch_number)
            .await;

        // If we weren't able to fetch all the data, we should unlock the batch before returning.
        match proof_generation_data {
            Ok(data) => Ok(Some(data)),
            Err(err) => {
                self.unlock_batch(l1_batch_number).await?;
                Err(err)
            }
        }
    }

    /// Will choose a batch that has all the required data and isn't picked up by any prover yet.
    pub async fn lock_batch_for_proving(
        &self,
        proof_generation_timeout: Duration,
    ) -> Result<Option<L1BatchNumber>, ProcessorError> {
        self.pool
            .connection()
            .await?
            .proof_generation_dal()
            .lock_batch_for_proving(proof_generation_timeout, self.proving_mode.clone().into())
            .await
            .map_err(Into::into)
    }

    /// Will choose a batch that has all the required data and isn't picked up by proving network yet.
    pub async fn lock_batch_for_proving_network(
        &self,
    ) -> Result<Option<L1BatchNumber>, ProcessorError> {
        self.pool
            .connection()
            .await?
            .proof_generation_dal()
            .lock_batch_for_proving_network()
            .await
            .map_err(Into::into)
    }

    /// Marks the batch as 'unpicked', allowing it to be picked up by another prover.
    pub async fn unlock_batch(&self, l1_batch_number: L1BatchNumber) -> Result<(), ProcessorError> {
        self.pool
            .connection()
            .await?
            .proof_generation_dal()
            .unlock_batch(l1_batch_number, self.proving_mode.clone().into())
            .await
            .map_err(Into::into)
    }

    pub async fn save_proof(
        &self,
        l1_batch_id: L1BatchId,
        proof: L1BatchProofForL1,
    ) -> Result<(), ProcessorError> {
        tracing::info!("Received proof for block number: {l1_batch_id}");

        let blob_url = self
            .blob_store
            .put(
                L1BatchProofForL1Key::Core((l1_batch_id.batch_number(), proof.protocol_version())),
                &proof,
            )
            .await?;

        let aggregation_coords = proof.aggregation_result_coords();

        let system_logs_hash_from_prover = H256::from_slice(&aggregation_coords[0]);
        let state_diff_hash_from_prover = H256::from_slice(&aggregation_coords[1]);
        let bootloader_heap_initial_content_from_prover = H256::from_slice(&aggregation_coords[2]);
        let events_queue_state_from_prover = H256::from_slice(&aggregation_coords[3]);

        let mut storage = self.pool.connection().await.unwrap();

        let l1_batch = storage
            .blocks_dal()
            .get_l1_batch_metadata(l1_batch_id.batch_number())
            .await?
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
            || bootloader_heap_initial_content != bootloader_heap_initial_content_from_prover
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
                    (log.0.key == H256::from_low_u64_be(STATE_DIFF_HASH_KEY_PRE_GATEWAY as u64))
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
            let server_values = format!(
                "system_logs_hash = {system_logs_hash}, state_diff_hash = {state_diff_hash}"
            );
            let prover_values = format!("system_logs_hash = {system_logs_hash_from_prover}, state_diff_hash = {state_diff_hash_from_prover}");
            panic!(
                "Auxilary output doesn't match, server values: {} prover values: {}",
                server_values, prover_values
            );
        }

        storage
            .proof_generation_dal()
            .save_proof_artifacts_metadata(l1_batch_id.batch_number(), &blob_url)
            .await?;
        Ok(())
    }

    pub async fn save_skipped_proof(&self, l1_batch_id: L1BatchId) -> Result<(), ProcessorError> {
        tracing::info!("Received skipped proof for block number: {l1_batch_id}");
        self.pool
            .connection()
            .await
            .unwrap()
            .proof_generation_dal()
            .mark_proof_generation_job_as_skipped(l1_batch_id.batch_number())
            .await
            .map_err(Into::into)
    }
}

impl Processor<Readonly> {
    pub async fn get_proof_generation_data(&self) -> Result<ProofGenerationData, ProcessorError> {
        tracing::debug!("Received request for proof generation data");
        let latest_available_batch = self.latest_available_batch().await?;
        self.proof_generation_data_for_existing_batch(latest_available_batch)
            .await
    }

    async fn latest_available_batch(&self) -> Result<L1BatchNumber, ProcessorError> {
        Ok(self
            .pool
            .connection()
            .await
            .unwrap()
            .proof_generation_dal()
            .get_latest_proven_batch()
            .await?)
    }

    pub async fn verify_proof(
        &self,
        l1_batch_number: L1BatchNumber,
        binary_proof: Vec<u8>,
        protocol_version: ProtocolSemanticVersion,
    ) -> Result<(), ProcessorError> {
        let expected_proof: L1BatchProofForL1<CBOR> = match self
            .blob_store
            .get(L1BatchProofForL1Key::Core((
                l1_batch_number,
                protocol_version,
            )))
            .await
        {
            Ok(proof) => proof,
            Err(_) => self
                .blob_store
                .get::<L1BatchProofForL1<Bincode>>(L1BatchProofForL1Key::Core((
                    l1_batch_number,
                    protocol_version,
                )))
                .await
                .map(Into::into)?,
        };

        if expected_proof.protocol_version() != protocol_version {
            return Err(ProcessorError::InvalidProof);
        }

        if <L1BatchProofForL1 as StoredObject>::serialize(&expected_proof)? != binary_proof {
            return Err(ProcessorError::InvalidProof);
        }

        Ok(())
    }
}
