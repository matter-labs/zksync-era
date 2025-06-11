use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::{Arc, ObjectStore};
use zksync_proof_data_handler::processor::{Processor, Readonly};
use zksync_prover_interface::api::ProofGenerationData;
use zksync_types::{L1BatchId, L2ChainId};

pub struct ReadinessChecker {
    processor: Processor<Readonly>,
    chain_id: L2ChainId,
    connection_pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
}

impl ReadinessChecker {
    pub fn new(
        processor: Processor<Readonly>,
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
        let batch_number = self.processor.get_oldest_not_proven_batch().await?;
        if batch_number.is_none() {
            return Ok(());
        }
        let batch_number = batch_number.unwrap();
        let batch_data = self
            .processor
            .proof_generation_data_for_existing_batch(batch_number)
            .await?;

        let blob_url = self
            .blob_store
            .put::<ProofGenerationData>(L1BatchId::new(self.chain_id, batch_number), &batch_data)
            .await?;

        self.connection_pool
            .connection()
            .await?
            .eth_proof_manager_dal()
            .insert_proof_request(batch_number, url)
            .await?;

        Ok(())
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                return Ok(());
            }
            self.check_readiness().await?;

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }
}
