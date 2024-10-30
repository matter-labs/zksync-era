use std::fmt;

use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1};
use zksync_basic_types::H256;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};
use zksync_prover_interface::inputs::TeeVerifierInput;
use zksync_tee_verifier::Verify;
use zksync_types::L1BatchNumber;

use crate::{
    api_client::TeeApiClient, config::TeeProverConfig, error::TeeProverError, metrics::METRICS,
};

/// Wiring layer for `TeeProver`
#[derive(Debug)]
pub(crate) struct TeeProverLayer {
    config: TeeProverConfig,
}

impl TeeProverLayer {
    pub fn new(config: TeeProverConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, IntoContext)]
pub(crate) struct LayerOutput {
    #[context(task)]
    pub tee_prover: TeeProver,
}

#[async_trait::async_trait]
impl WiringLayer for TeeProverLayer {
    type Input = ();
    type Output = LayerOutput;

    fn layer_name(&self) -> &'static str {
        "tee_prover_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let api_url = self.config.api_url.clone();
        let tee_prover = TeeProver {
            config: self.config,
            api_client: TeeApiClient::new(api_url),
        };
        Ok(LayerOutput { tee_prover })
    }
}

pub(crate) struct TeeProver {
    config: TeeProverConfig,
    api_client: TeeApiClient,
}

impl fmt::Debug for TeeProver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeeProver")
            .field("config", &self.config)
            .finish()
    }
}

impl TeeProver {
    fn verify(
        &self,
        tvi: TeeVerifierInput,
    ) -> Result<(Signature, L1BatchNumber, H256), TeeProverError> {
        match tvi {
            TeeVerifierInput::V1(tvi) => {
                let observer = METRICS.proof_generation_time.start();
                let verification_result = tvi.verify().map_err(TeeProverError::Verification)?;
                let root_hash_bytes = verification_result.value_hash.as_bytes();
                let batch_number = verification_result.batch_number;
                let msg_to_sign = Message::from_slice(root_hash_bytes)
                    .map_err(|e| TeeProverError::Verification(e.into()))?;
                let signature = self.config.signing_key.sign_ecdsa(msg_to_sign);
                observer.observe();
                Ok((signature, batch_number, verification_result.value_hash))
            }
            _ => Err(TeeProverError::Verification(anyhow::anyhow!(
                "Only TeeVerifierInput::V1 verification supported."
            ))),
        }
    }

    async fn step(&self, public_key: &PublicKey) -> Result<Option<L1BatchNumber>, TeeProverError> {
        match self.api_client.get_job(self.config.tee_type).await? {
            Some(job) => {
                let (signature, batch_number, root_hash) = self.verify(*job)?;
                self.api_client
                    .submit_proof(
                        batch_number,
                        signature,
                        public_key,
                        root_hash,
                        self.config.tee_type,
                    )
                    .await?;
                Ok(Some(batch_number))
            }
            None => {
                tracing::trace!("There are currently no pending batches to be proven");
                Ok(None)
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

        let config = &self.config;
        let attestation_quote_bytes = std::fs::read(&config.attestation_quote_file_path)?;
        let public_key = config.signing_key.public_key(&Secp256k1::new());
        self.api_client
            .register_attestation(attestation_quote_bytes, &public_key)
            .await?;

        let mut retries = 1;
        let mut backoff = config.initial_retry_backoff();
        let mut observer = METRICS.job_waiting_time.start();

        loop {
            if *stop_receiver.0.borrow() {
                tracing::info!("Stop signal received, shutting down TEE Prover component");
                return Ok(());
            }
            let result = self.step(&public_key).await;
            let need_to_sleep = match result {
                Ok(batch_number) => {
                    retries = 1;
                    backoff = config.initial_retry_backoff();
                    if let Some(batch_number) = batch_number {
                        observer.observe();
                        observer = METRICS.job_waiting_time.start();
                        METRICS
                            .last_batch_number_processed
                            .set(batch_number.0 as u64);
                        false
                    } else {
                        true
                    }
                }
                Err(err) => {
                    METRICS.network_errors_counter.inc_by(1);
                    if !err.is_retriable() || retries > config.max_retries {
                        return Err(err.into());
                    }
                    tracing::warn!(%err, "Failed TEE prover step function {retries}/{}, retrying in {} milliseconds.", config.max_retries, backoff.as_millis());
                    retries += 1;
                    backoff = std::cmp::min(
                        backoff.mul_f32(config.retry_backoff_multiplier),
                        config.max_backoff(),
                    );
                    true
                }
            };
            if need_to_sleep {
                tokio::time::timeout(backoff, stop_receiver.0.changed())
                    .await
                    .ok();
            }
        }
    }
}
