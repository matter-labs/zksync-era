use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::{Arc, ObjectStore};
use zksync_proof_data_handler::processor::{Locking, Processor};
use zksync_prover_interface::api::ProofGenerationData;
use zksync_types::{L1BatchId, L2ChainId};

pub struct ReadinessChecker {
    processor: Processor<Locking>,
    chain_id: L2ChainId,
    connection_pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
}

impl ReadinessChecker {
    pub fn new(
        processor: Processor<Locking>,
        chain_id: L2ChainId,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
    ) -> Self {
        Self {
            processor,
            chain_id,
            connection_pool,
            blob_store,
        }
    }

    pub async fn check_readiness(&self) -> anyhow::Result<()> {
        if let Some(proof_generation_data) = self.processor.get_proof_generation_data().await? {
            let blob_url = self
                .blob_store
                .put::<ProofGenerationData>(
                    L1BatchId::new(self.chain_id, proof_generation_data.l1_batch_number),
                    &proof_generation_data,
                )
                .await?;

            self.connection_pool
                .connection()
                .await?
                .eth_proof_manager_dal()
                .insert_proof_request(proof_generation_data.l1_batch_number, url)
                .await?;
        }

        Ok(())
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                return Ok(());
            }
            let amount = self
                .connection_pool
                .connection()
                .await?
                .proof_generation_dal()
                // todo: these timeouts should be configurable
                .fallback_stuck_batches_to_prover_cluster(
                    Duration::from_hours(3),
                    Duration::from_mins(20),
                    Duration::from_mins(2),
                )
                .await?;

            if amount > 0 {
                tracing::info!("{} batches were stuck and were routed to prover cluster instead of being proven", amount);
            }

            self.check_readiness().await?;

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }
}
