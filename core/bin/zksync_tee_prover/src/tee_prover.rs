use std::time::Duration;

use reqwest::Client;
use secp256k1::{ecdsa::Signature, Message, Secp256k1, SecretKey};
use url::Url;
use zksync_basic_types::H256;
use zksync_node_framework::{
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};
use zksync_prover_interface::inputs::TeeVerifierInput;
use zksync_tee_verifier::Verifiable;
use zksync_types::{tee_types::TeeType, L1BatchNumber};

use crate::api_client::ApiClient;

/// Wiring layer for [`TeeProver`]
///
/// ## Requests resources
///
/// no resources requested
///
/// ## Adds tasks
///
/// - [`TeeProver`]
#[derive(Debug)]
pub struct TeeProverLayer {
    api_url: Url,
    signing_key: SecretKey,
    attestation_quote_bytes: Vec<u8>,
    tee_type: TeeType,
}

impl TeeProverLayer {
    pub fn new(
        api_url: Url,
        signing_key: SecretKey,
        attestation_quote_bytes: Vec<u8>,
        tee_type: TeeType,
    ) -> Self {
        Self {
            api_url,
            signing_key,
            attestation_quote_bytes,
            tee_type,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for TeeProverLayer {
    fn layer_name(&self) -> &'static str {
        "tee_prover_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let tee_prover_task = TeeProver {
            signing_key: self.signing_key,
            attestation_quote_bytes: self.attestation_quote_bytes,
            tee_type: self.tee_type,
            api_client: ApiClient::new(self.api_url, Client::new()),
        };
        context.add_task(Box::new(tee_prover_task));
        Ok(())
    }
}

struct TeeProver {
    signing_key: SecretKey,
    attestation_quote_bytes: Vec<u8>,
    tee_type: TeeType,
    api_client: ApiClient,
}

impl TeeProver {
    fn verify(&self, tvi: TeeVerifierInput) -> anyhow::Result<(Signature, L1BatchNumber, H256)> {
        match tvi.verify() {
            Err(e) => {
                let err_msg = format!("L1 batch verification failed: {e}");
                tracing::warn!(err_msg);
                Err(anyhow::anyhow!(err_msg))
            }
            Ok(verification_result) => {
                let root_hash_bytes = verification_result.0.as_bytes();
                let batch_number = verification_result.1;
                let msg_to_sign = Message::from_slice(root_hash_bytes)?;
                let signature = self.signing_key.sign_ecdsa(msg_to_sign);
                Ok((signature, batch_number, verification_result.0))
            }
        }
    }
}

#[async_trait::async_trait]
impl Task for TeeProver {
    fn id(&self) -> TaskId {
        "tee_prover".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        tracing::info!("Starting the task {}", self.id());

        self.api_client
            .register_attestation(self.attestation_quote_bytes.clone(), &self.signing_key)
            .await?;

        const POLLING_INTERVAL_MS: u64 = 1000;
        const MAX_BACKOFF_MS: u64 = 60_000;
        const BACKOFF_MULTIPLIER: u64 = 2;

        let mut backoff: u64 = POLLING_INTERVAL_MS;

        loop {
            if *stop_receiver.0.borrow() {
                tracing::warn!("Stop signal received, shutting down TEE Prover component");
                return Ok(());
            }
            let job = match self.api_client.get_job().await {
                Ok(Some(job)) => {
                    backoff = POLLING_INTERVAL_MS;
                    job
                }
                Ok(None) => {
                    tracing::info!("There are currently no pending batches to be proven; backing off for {} ms", backoff);
                    tokio::time::timeout(Duration::from_millis(backoff), stop_receiver.0.changed())
                        .await
                        .ok();
                    backoff = (backoff * BACKOFF_MULTIPLIER).min(MAX_BACKOFF_MS);
                    continue;
                }
                Err(e) => return Err(e),
            };
            let (signature, batch_number, root_hash) = self.verify(*job)?;
            let secp = Secp256k1::new();
            let pubkey = self.signing_key.public_key(&secp);
            self.api_client
                .submit_proof(batch_number, signature, &pubkey, root_hash, self.tee_type)
                .await?;
        }
    }
}
