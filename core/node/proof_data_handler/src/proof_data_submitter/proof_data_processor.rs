use std::sync::Arc;

use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::{
    api::ProofGenerationData,
    inputs::{
        L1BatchMetadataHashes, VMRunWitnessInputData, WitnessInputData, WitnessInputMerklePaths,
    },
};
use zksync_types::{basic_fri_types::Eip4844Blobs, commitment::L1BatchCommitmentMode, L1BatchNumber, L2ChainId};

use crate::metrics::METRICS;

#[derive(Debug)]
pub struct ProofGenerationDataProcessor {
    chain_id: L2ChainId,
    pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    commitment_mode: L1BatchCommitmentMode,
}

impl ProofGenerationDataProcessor {
    pub fn new(
        chain_id: L2ChainId,
        pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            chain_id,
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
            chain_id: self.chain_id,
            l1_batch_number,
            witness_input_data: blob,
            protocol_version: protocol_version.version,
            l1_verifier_config: protocol_version.l1_verifier_config,
        })
    }
}
