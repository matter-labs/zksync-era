use std::{sync::Arc, time::Duration};

use jsonrpsee::{
    core::{async_trait, RpcResult, SubscriptionResult},
    types::{error::INTERNAL_ERROR_CODE, ErrorObject},
    PendingSubscriptionSink, SubscriptionMessage, TrySendError,
};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_interface::{
    api::{ProofGenerationData, SubmitProofRequest},
    outputs::L1BatchProofForL1,
    rpc::GatewayRpcServer,
};
use zksync_types::{
    prover_dal::ProofCompressionJobStatus, ChainAwareL1BatchNumber, L1BatchNumber, L2ChainId,
};

pub struct RpcDataProcessor {
    pool: ConnectionPool<Prover>,
    blob_store: Arc<dyn ObjectStore>,
}

impl RpcDataProcessor {
    pub fn new(pool: ConnectionPool<Prover>, blob_store: Arc<dyn ObjectStore>) -> Self {
        Self { pool, blob_store }
    }

    pub async fn subscribe(&self, chain_id: L2ChainId, pending: PendingSubscriptionSink) {
        let Ok(mut sink) = pending.accept().await else {
            return;
        };

        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;

            let (l1_batch_number, request) = match self.next_submit_proof_request(chain_id).await {
                Some(data) => data,
                None => {
                    tracing::info!("No proofs to send, waiting for new ones");
                    continue;
                }
            };

            let msg = SubscriptionMessage::from_json(&request).unwrap();
            match sink.try_send(msg) {
                Ok(_) => {
                    tracing::info!(
                        "Proof for chain {}, batch {:?} was sent to client",
                        chain_id.as_u64(),
                        l1_batch_number
                    );
                }
                Err(TrySendError::Closed(_)) => break,
                Err(TrySendError::Full(_)) => {
                    tracing::warn!("Channel is full, waiting until it's ready");
                }
            }
        }
    }

    pub async fn next_submit_proof_request(
        &self,
        chain_id: L2ChainId,
    ) -> Option<(L1BatchNumber, SubmitProofRequest)> {
        let (l1_batch_number, protocol_version, status) = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .get_least_proven_block_not_sent_to_server(chain_id)
            .await?;

        let batch_id = ChainAwareL1BatchNumber::new(chain_id, l1_batch_number);

        let request = match status {
            ProofCompressionJobStatus::Successful => {
                let proof = self
                    .blob_store
                    .get::<L1BatchProofForL1>((batch_id, protocol_version))
                    .await
                    .expect("Failed to get compressed snark proof from blob store");
                SubmitProofRequest::Proof(l1_batch_number, Box::new(proof))
            }
            ProofCompressionJobStatus::Skipped => {
                SubmitProofRequest::SkippedProofGeneration(l1_batch_number)
            }
            _ => panic!(
                "Trying to send proof that are not successful status: {:?}",
                status
            ),
        };

        Some((l1_batch_number, request))
    }

    pub async fn save_successful_sent_proof(
        &self,
        chain_id: L2ChainId,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        self.pool
            .connection()
            .await?
            .fri_proof_compressor_dal()
            .mark_proof_sent_to_server(ChainAwareL1BatchNumber::new(chain_id, l1_batch_number))
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn save_proof_gen_data(
        &self,
        chain_id: L2ChainId,
        data: ProofGenerationData,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Received proof generation data for batch: {:?}, chain {}",
            data.l1_batch_number,
            chain_id.as_u64(),
        );

        let store = &*self.blob_store;
        let witness_inputs = store
            .put(
                ChainAwareL1BatchNumber::new(chain_id, data.l1_batch_number),
                &data.witness_input_data,
            )
            .await?;
        let mut connection = self.pool.connection().await?;

        connection
            .fri_protocol_versions_dal()
            .save_prover_protocol_version(data.protocol_version, data.l1_verifier_config)
            .await?;

        connection
            .fri_basic_witness_generator_dal()
            .save_witness_inputs(
                ChainAwareL1BatchNumber::new(chain_id, data.l1_batch_number),
                &witness_inputs,
                data.protocol_version,
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl GatewayRpcServer for RpcDataProcessor {
    async fn submit_proof_generation_data(
        &self,
        chain_id: L2ChainId,
        data: ProofGenerationData,
    ) -> RpcResult<()> {
        self.save_proof_gen_data(chain_id, data)
            .await
            .map_err(|err| {
                ErrorObject::owned(INTERNAL_ERROR_CODE, format!("{err:?}"), None::<()>)
            })?;
        Ok(())
    }

    async fn received_final_proof(
        &self,
        chain_id: L2ChainId,
        l1_batch_number: L1BatchNumber,
    ) -> RpcResult<()> {
        tracing::info!(
            "Received confirmation of successfully sent proof for batch {:?}, chain id {:?}",
            l1_batch_number,
            chain_id.as_u64(),
        );
        self.save_successful_sent_proof(chain_id, l1_batch_number)
            .await
            .map_err(|err| ErrorObject::owned(INTERNAL_ERROR_CODE, format!("{err:?}"), None::<()>))
    }

    async fn subscribe_for_proofs(
        &self,
        subscription_sink: PendingSubscriptionSink,
        chain_id: L2ChainId,
    ) -> SubscriptionResult {
        self.subscribe(chain_id, subscription_sink).await;
        Ok(())
    }
}
