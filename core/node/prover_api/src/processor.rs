use crate::error::ProcessorError;
use axum::Json;
use std::sync::Arc;
use zksync_basic_types::commitment::L1BatchCommitmentMode;
use zksync_basic_types::L1BatchNumber;
use zksync_config::configs::prover_api::ProverApiConfig;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_proof_data_handler::proof_generation_data_for_existing_batch_impl;
use zksync_prover_interface::api::{
    ProofGenerationData, ProofGenerationDataRequest, ProofGenerationDataResponse,
};

pub(crate) struct Processor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    config: ProverApiConfig,
    commitment_mode: L1BatchCommitmentMode,
    last_available_batch_data: Option<Box<ProofGenerationData>>,
}

impl Processor {
    pub(crate) async fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Core>,
        config: ProverApiConfig,
        commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            blob_store,
            pool,
            config,
            commitment_mode,
            last_available_batch_data: None,
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) async fn get_proof_generation_data(
        &mut self,
        request: Json<ProofGenerationDataRequest>,
    ) -> Result<Json<ProofGenerationDataResponse>, ProcessorError> {
        tracing::info!("Received request for proof generation data: {:?}", request);

        let l1_batch_number = self
            .pool
            .connection()
            .await
            .unwrap()
            .proof_generation_dal()
            .get_available_batch()
            .await?
            .unwrap_or(L1BatchNumber(self.config.last_available_batch));

        if let Some(data) = &self.last_available_batch_data {
            if data.l1_batch_number == l1_batch_number {
                return Ok(Json(ProofGenerationDataResponse::Success(Some(
                    data.clone(),
                ))));
            }
        }

        let proof_generation_data = proof_generation_data_for_existing_batch_impl(
            self.blob_store.clone(),
            self.pool.clone(),
            self.commitment_mode,
            l1_batch_number,
        )
        .await;

        match proof_generation_data {
            Ok(data) => {
                self.last_available_batch_data = Some(Box::new(data.clone()));
                Ok(Json(ProofGenerationDataResponse::Success(Some(Box::new(
                    data,
                )))))
            }
            Err(err) => Err(err),
        }
    }

    pub(crate) async fn verify_proof(
        &self,
        proof: Vec<u8>,
        l1_batch_number: L1BatchNumber,
    ) -> Result<(), ProcessorError> {
        unimplemented!()
    }
}
