use std::{collections::HashMap, sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::{
    circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver,
    ProverServiceDataKey,
};
use zksync_prover_job_processor::JobPicker;
use zksync_types::prover_dal::FriProverJobMetadata;

use crate::{
    metrics::WITNESS_VECTOR_GENERATOR_METRICS,
    types::witness_vector_generator_payload::WitnessVectorGeneratorPayload,
    witness_vector_generator::{
        witness_vector_generator_metadata_loader::WitnessVectorMetadataLoader,
        WitnessVectorGeneratorExecutor,
    },
};

/// WitnessVectorGenerator job picker implementation.
/// Picks job from database (via MetadataLoader) and gets data from object store.
#[derive(Debug)]
pub struct WitnessVectorGeneratorJobPicker<ML: WitnessVectorMetadataLoader> {
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
    metadata_loader: ML,
}

impl<ML: WitnessVectorMetadataLoader> WitnessVectorGeneratorJobPicker<ML> {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        object_store: Arc<dyn ObjectStore>,
        finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
        metadata_loader: ML,
    ) -> Self {
        Self {
            connection_pool,
            object_store,
            finalization_hints_cache,
            metadata_loader,
        }
    }
}

#[async_trait]
impl<ML: WitnessVectorMetadataLoader> JobPicker for WitnessVectorGeneratorJobPicker<ML> {
    type ExecutorType = WitnessVectorGeneratorExecutor;
    async fn pick_job(
        &mut self,
    ) -> anyhow::Result<Option<(WitnessVectorGeneratorPayload, FriProverJobMetadata)>> {
        let start_time = Instant::now();
        tracing::info!("Started picking witness vector generator job");
        let connection = self
            .connection_pool
            .connection()
            .await
            .context("failed to get db connection")?;
        let metadata = match self.metadata_loader.load_metadata(connection).await {
            None => return Ok(None),
            Some(metadata) => metadata,
        };

        let circuit_wrapper = self
            .object_store
            .get(metadata.into())
            .await
            .context("failed to get circuit_wrapper from object store")?;

        let key = ProverServiceDataKey {
            circuit_id: metadata.circuit_id,
            stage: metadata.aggregation_round.into(),
        }
        .crypto_setup_key();
        let finalization_hints = self
            .finalization_hints_cache
            .get(&key)
            .context("failed to retrieve finalization key from cache")?
            .clone();

        let payload = WitnessVectorGeneratorPayload {
            circuit_wrapper,
            finalization_hints,
        };
        tracing::info!(
            "Finished picking witness vector generator job {}, on batch {}, for circuit {}, at round {} in {:?}",
            metadata.id,
            metadata.batch_id,
            metadata.circuit_id,
            metadata.aggregation_round,
            start_time.elapsed()
        );
        WITNESS_VECTOR_GENERATOR_METRICS
            .pick_time
            .observe(start_time.elapsed());
        Ok(Some((payload, metadata)))
    }
}
