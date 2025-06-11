use std::{sync::Arc, time::Duration};

use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::clients::L1;
use zksync_object_store::ObjectStore;
use zksync_prover_interface::inputs::WitnessInputData;
use zksync_types::L2ChainId;

use crate::client::EthProofManagerClient;

pub struct EthProofSender {
    chain_id: L2ChainId,
    client: EthProofManagerClient<L1>,
    connection_pool: ConnectionPool<Core>,
    object_store: Arc<dyn ObjectStore>,
}

impl EthProofSender {
    pub fn new(
        chain_id: L2ChainId,
        client: EthProofManagerClient<L1>,
        connection_pool: ConnectionPool<Core>,
        object_store: Arc<dyn ObjectStore>,
    ) -> Self {
        Self {
            chain_id,
            client,
            connection_pool,
            object_store,
        }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth proof sender is shutting down");
                return Ok(());
            }

            // check ready to be proven batches
            if let Some((batch_number, proof_gen_data_blob_url)) = self
                .connection_pool
                .connection()
                .await?
                .eth_proof_manager_dal()
                .get_ready_to_be_proven_batches()
                .await?
            {
                let witness_input_data: WitnessInputData =
                    self.object_store.get(proof_gen_data_blob_url).await?;

                self.client
                    .submit_proof_request(self.chain_id, batch_number, witness_input_data)
                    .await?;
                self.connection_pool
                    .connection()
                    .await?
                    .eth_proof_manager_dal()
                    .mark_proof_request_as_sent_to_l1(batch_number)
                    .await?;
            }

            // check validated not sent batches
            if let Some((batch_number, validation_status)) = self
                .connection_pool
                .connection()
                .await?
                .eth_proof_manager_dal()
                .get_validated_not_sent_batch()
                .await?
            {
                self.client
                    .submit_proof_validation_result(self.chain_id, batch_number, validation_status)
                    .await?;
            }

            tokio::time::timeout(Duration::from_secs(10), stop_receiver.changed())
                .await
                .ok();
        }
    }
}
