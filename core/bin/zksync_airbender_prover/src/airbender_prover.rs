use std::fmt;

use zksync_basic_types::{L1BatchNumber, H256};
use zksync_crypto_primitives::{sign, K256PrivateKey, Signature};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};
use zksync_airbender_prover_interface::inputs::AirbenderVerifierInput;
use zksync_airbender_verifier::Verify;

use crate::{
    api_client::AirbenderApiClient, config::AirbenderProverConfig, error::AirbenderProverError, metrics::METRICS,
};

/// Wiring layer for `AirbenderProver`
#[derive(Debug)]
pub(crate) struct AirbenderProverLayer {
    config: AirbenderProverConfig,
}

impl AirbenderProverLayer {
    pub fn new(config: AirbenderProverConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, IntoContext)]
pub(crate) struct LayerOutput {
    #[context(task)]
    pub airbender_prover: AirbenderProver,
}

#[async_trait::async_trait]
impl WiringLayer for AirbenderProverLayer {
    type Input = ();
    type Output = LayerOutput;

    fn layer_name(&self) -> &'static str {
        "airbender_prover_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let api_url = self.config.prover_api.api_url.clone();
        let airbender_prover = AirbenderProver {
            config: self.config,
            api_client: AirbenderApiClient::new(api_url),
        };
        Ok(LayerOutput { airbender_prover })
    }
}

pub(crate) struct AirbenderProver {
    config: AirbenderProverConfig,
    api_client: AirbenderApiClient,
}

impl fmt::Debug for AirbenderProver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AirbenderProver")
            .field("config", &self.config)
            .finish()
    }
}

impl AirbenderProver {
    /// Signs the message in Ethereum-compatible format for on-chain verification.
    pub fn sign_message(&self, message: &H256) -> Result<Signature, AirbenderProverError> {
        let private_key: K256PrivateKey = self.config.sig_conf.signing_key.into();
        let signature =
            sign(&private_key, message).map_err(|e| AirbenderProverError::Verification(e.into()))?;
        Ok(signature)
    }

    fn verify(
        &self,
        tvi: AirbenderVerifierInput,
    ) -> Result<(L1BatchNumber, Vec<u8>), AirbenderProverError> {
        match tvi {
            AirbenderVerifierInput::V1(tvi) => {
                let observer = METRICS.proof_generation_time.start();
                let verification_result = tvi.verify().map_err(AirbenderProverError::Verification)?;
                let batch_number = verification_result.batch_number;
                let duration = observer.observe();
                tracing::info!(
                    proof_generation_time = duration.as_secs_f64(),
                    l1_batch_number = %batch_number,
                    l1_root_hash = ?verification_result.value_hash,
                    "L1 batch verified",
                );
                Ok((batch_number, verification_result.value_hash.as_bytes().to_vec()))
            }
            _ => Err(AirbenderProverError::Verification(anyhow::anyhow!(
                "Only AirbenderVerifierInput::V1 verification supported."
            ))),
        }
    }

    async fn step(&self) -> Result<Option<L1BatchNumber>, AirbenderProverError> {
        match self.api_client.get_job().await {
            Ok(Some(job)) => {
                let (batch_number, proof) = self.verify(job)?;
                self.api_client
                    .submit_proof(batch_number, proof)
                    .await?;
                Ok(Some(batch_number))
            }
            Ok(None) => {
                tracing::trace!("There are currently no pending batches to be proven");
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }
}

#[async_trait::async_trait]
impl Task for AirbenderProver {
    fn id(&self) -> TaskId {
        "airbender_prover".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        tracing::info!("Starting the task {}", self.id());

        let config = &self.config.prover_api;

        let mut retries = 1;
        let mut backoff = config.initial_retry_backoff;
        let mut observer = METRICS.job_waiting_time.start();

        loop {
            if *stop_receiver.0.borrow() {
                tracing::info!("Stop request received, shutting down Airbender Prover component");
                return Ok(());
            }
            let result = self.step().await;
            let need_to_sleep = match result {
                Ok(batch_number) => {
                    retries = 1;
                    backoff = config.initial_retry_backoff;
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
                    METRICS.network_errors_counter.inc();
                    if !err.is_retriable() || retries > config.max_retries {
                        return Err(err.into());
                    }
                    tracing::warn!(%err, "Failed Airbender prover step function {retries}/{}, retrying in {} milliseconds.", config.max_retries, backoff.as_millis());
                    retries += 1;
                    backoff = std::cmp::min(
                        backoff.mul_f32(config.retry_backoff_multiplier),
                        config.max_backoff,
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use secp256k1::SecretKey;
    use url::Url;
    use zksync_crypto_primitives::{public_to_address, recover};

    use super::*;
    use crate::config::{AirbenderProverApiConfig, AirbenderProverSigConfig};

    #[test]
    fn test_recover() {
        let signing_key = SecretKey::from_slice(
            &hex::decode("c87509a1c067bbde78beb793e6fa76530b6382a4c0241e5e4a9ec0a0f44dc0d3")
                .unwrap(),
        )
        .unwrap();
        let airbender_prover_config = AirbenderProverConfig {
            sig_conf: AirbenderProverSigConfig {
                signing_key,
            },
            prover_api: AirbenderProverApiConfig {
                api_url: Url::parse("http://mock").unwrap(),
                max_retries: 5,
                initial_retry_backoff: Duration::from_secs(1),
                retry_backoff_multiplier: 2.0,
                max_backoff: Duration::from_secs(128),
            },
        };
        let airbender_prover = AirbenderProver {
            config: airbender_prover_config,
            api_client: AirbenderApiClient::new(Url::parse("http://mock").unwrap()),
        };
        let private_key: K256PrivateKey = signing_key.into();
        let expected_address = "0x627306090abaB3A6e1400e9345bC60c78a8BEf57"
            .parse()
            .unwrap();
        assert_eq!(private_key.address(), expected_address);

        // Generate a random root hash, create a message from the hash, and sign the message using
        // the secret key
        let random_root_hash = H256::random();
        let signature = airbender_prover.sign_message(&random_root_hash).unwrap();

        // Recover the signer's Ethereum address from the signature and the message, and verify it
        // matches the expected address
        let recovered_pubkey = recover(&signature, &random_root_hash).unwrap();
        let proof_address = public_to_address(&recovered_pubkey);

        assert_eq!(proof_address, expected_address);
    }
}
