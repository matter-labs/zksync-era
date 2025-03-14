use std::time::Duration;

use jsonrpsee::{async_client::Client, ws_client::WsClientBuilder};
use tokio::sync::watch;
use zksync_prover_interface::{api::SubmitProofRequest, rpc::GatewayRpcClient};

use crate::rpc_client::processor::ProofDataProcessor;

pub mod processor;

#[derive(Debug)]
pub struct RpcClient {
    processor: ProofDataProcessor,
    ws_url: String,
    readiness_check_interval: Duration,
    connection_retry_interval: Duration,
}

impl RpcClient {
    pub fn new(
        processor: ProofDataProcessor,
        ws_url: String,
        readiness_check_interval: Duration,
        connection_retry_interval: Duration,
    ) -> Self {
        Self {
            processor,
            ws_url,
            readiness_check_interval,
            connection_retry_interval,
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let proof_data_sender = self.run_and_maintain_proof_data_submitter(stop_receiver.clone());
        let proof_receiver = self.run_and_maintain_proof_receiver(stop_receiver.clone());

        tracing::info!("Starting proof data submitter and receiver");

        tokio::select! {
            _ = proof_data_sender => {
                tracing::info!("Proof data submitter stopped");
            }
            _ = proof_receiver => {
                tracing::info!("Proof receiver stopped");
            }
        }

        Ok(())
    }

    async fn run_and_maintain_proof_data_submitter(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        loop {
            tokio::time::sleep(self.connection_retry_interval).await;
            if *stop_receiver.borrow() {
                tracing::warn!("Stop signal received, shutting down proof data submitter");
                return Ok(());
            }

            tracing::info!(
                "Connecting to the server for proof data submitter by URL: {}",
                self.ws_url
            );

            let client = WsClientBuilder::default().build(&self.ws_url).await;
            if let Err(e) = client {
                tracing::error!(
                    "Failed to connect to the server for proof data submitter: {}, sleeping for {:?}",
                    e,
                    self.connection_retry_interval
                );
                continue;
            }

            tracing::info!(
                "Established long living connection with gateway for proof data submitter by URL: {}",
                self.ws_url
            );

            if let Err(e) = self
                .run_proof_data_submitter(client?, stop_receiver.clone())
                .await
            {
                tracing::error!("Proof data submitter failed: {}", e);
            }
        }
    }

    async fn run_proof_data_submitter(
        &self,
        client: Client,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        loop {
            tokio::time::sleep(self.readiness_check_interval).await;
            if *stop_receiver.borrow() {
                tracing::warn!("Stop signal received, shutting down proof data submitter");
                return Ok(());
            }
            if !client.is_connected() {
                tracing::error!("Connection to the server is lost, trying to reconnect");
                return Err(anyhow::anyhow!("Connection to the server is lost"));
            }

            let Some(data) = self.processor.get_proof_generation_data().await? else {
                tracing::info!("No proof generation data to send, waiting for new batches");
                continue;
            };

            let l1_batch_number = data.l1_batch_number;

            tracing::info!("Sending proof for batch {:?}", l1_batch_number);

            if let Err(e) = client.submit_proof_generation_data(data).await {
                tracing::error!(
                    "Failed to submit proof generation data for batch {:?}, unlocking: {}",
                    e,
                    l1_batch_number
                );
                self.processor.unlock_batch(l1_batch_number).await?;
            } else {
                tracing::info!(
                    "Proof generation data for batch {:?} was sent successfully",
                    l1_batch_number
                );
            }
        }
    }

    async fn run_and_maintain_proof_receiver(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::warn!("Stop signal received, shutting down proof receiver");
                return Ok(());
            }

            tracing::info!(
                "Connecting to the server for proof data submitter by URL: {}",
                self.ws_url
            );

            let client = WsClientBuilder::default().build(&self.ws_url).await;
            if let Err(e) = client {
                tracing::error!(
                    "Failed to connect to the server for proof receiver: {}, retrying in {:?}",
                    e,
                    self.connection_retry_interval
                );
                tokio::time::sleep(self.connection_retry_interval).await;
                continue;
            }

            tracing::info!(
                "Established long living connection with gateway for proof receiver by URL: {}",
                self.ws_url
            );

            if let Err(e) = self
                .run_proof_receiver(client?, stop_receiver.clone())
                .await
            {
                tracing::error!("Proof data receiver failed: {}", e);
            }
        }
    }

    async fn run_proof_receiver(
        &self,
        client: Client,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let mut subscription = client.subscribe_for_proofs().await?;
        loop {
            if *stop_receiver.borrow() {
                tracing::warn!("Stop signal received, shutting down proof data receiver");
                return Ok(());
            }

            let proof = match subscription.next().await {
                Some(proof) => proof?,
                None => {
                    tracing::warn!("Proof subscription ended, needs resubscribing");
                    return Err(anyhow::anyhow!("Proof subscription ended"));
                }
            };

            let l1_batch_number = match proof {
                SubmitProofRequest::Proof(l1_batch_number, _) => l1_batch_number,
                SubmitProofRequest::SkippedProofGeneration(l1_batch_number) => l1_batch_number,
            };

            tracing::info!("Received proof for batch {:?}", l1_batch_number);

            self.processor.handle_proof(proof).await?;
            client.received_final_proof(l1_batch_number).await?
        }
    }
}
