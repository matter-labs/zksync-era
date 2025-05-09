use std::fmt;

use secp256k1::{PublicKey, Secp256k1};
use zksync_basic_types::{L1BatchNumber, H256};
use zksync_crypto_primitives::{sign, K256PrivateKey, Signature};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};
use zksync_tee_prover_interface::inputs::TeeVerifierInput;
use zksync_tee_verifier::Verify;

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
        let api_url = self.config.prover_api.api_url.clone();
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
    /// Signs the message in Ethereum-compatible format for on-chain verification.
    pub fn sign_message(&self, message: &H256) -> Result<Signature, TeeProverError> {
        let private_key: K256PrivateKey = self.config.sig_conf.signing_key.into();
        let signature =
            sign(&private_key, message).map_err(|e| TeeProverError::Verification(e.into()))?;
        Ok(signature)
    }

    fn verify(
        &self,
        tvi: TeeVerifierInput,
    ) -> Result<(Signature, L1BatchNumber, H256), TeeProverError> {
        match tvi {
            TeeVerifierInput::V1(tvi) => {
                let observer = METRICS.proof_generation_time.start();
                let verification_result = tvi.verify().map_err(TeeProverError::Verification)?;
                let batch_number = verification_result.batch_number;
                let signature = self.sign_message(&verification_result.value_hash)?;
                let duration = observer.observe();
                tracing::info!(
                    proof_generation_time = duration.as_secs_f64(),
                    l1_batch_number = %batch_number,
                    l1_root_hash = ?verification_result.value_hash,
                    "L1 batch verified",
                );
                Ok((signature, batch_number, verification_result.value_hash))
            }
            _ => Err(TeeProverError::Verification(anyhow::anyhow!(
                "Only TeeVerifierInput::V1 verification supported."
            ))),
        }
    }

    async fn step(&self, public_key: &PublicKey) -> Result<Option<L1BatchNumber>, TeeProverError> {
        match self.api_client.get_job(self.config.sig_conf.tee_type).await {
            Ok(Some(job)) => {
                let (signature, batch_number, root_hash) = self.verify(job)?;
                self.api_client
                    .submit_proof(
                        batch_number,
                        signature.into_electrum(),
                        public_key,
                        root_hash,
                        self.config.sig_conf.tee_type,
                    )
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
impl Task for TeeProver {
    fn id(&self) -> TaskId {
        "tee_prover".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        tracing::info!("Starting the task {}", self.id());

        let config = &self.config.prover_api;
        let attestation_quote_bytes =
            std::fs::read(&self.config.sig_conf.attestation_quote_file_path)?;
        let public_key = self
            .config
            .sig_conf
            .signing_key
            .public_key(&Secp256k1::new());
        self.api_client
            .register_attestation(attestation_quote_bytes, &public_key)
            .await?;

        let mut retries = 1;
        let mut backoff = config.initial_retry_backoff();
        let mut observer = METRICS.job_waiting_time.start();

        loop {
            if *stop_receiver.0.borrow() {
                tracing::info!("Stop request received, shutting down TEE Prover component");
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
                    METRICS.network_errors_counter.inc();
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use secp256k1::SecretKey;
    use url::Url;
    use zksync_basic_types::{self, tee_types::TeeType};
    use zksync_crypto_primitives::{public_to_address, recover};

    use super::*;
    use crate::config::{TeeProverApiConfig, TeeProverSigConfig};

    #[test]
    fn test_recover() {
        let signing_key = SecretKey::from_slice(
            &hex::decode("c87509a1c067bbde78beb793e6fa76530b6382a4c0241e5e4a9ec0a0f44dc0d3")
                .unwrap(),
        )
        .unwrap();
        let tee_prover_config = TeeProverConfig {
            sig_conf: TeeProverSigConfig {
                signing_key,
                attestation_quote_file_path: PathBuf::from("/tmp/mock"),
                tee_type: TeeType::Sgx,
            },
            prover_api: TeeProverApiConfig {
                api_url: Url::parse("http://mock").unwrap(),
                max_retries: TeeProverApiConfig::default_max_retries(),
                initial_retry_backoff_sec: TeeProverApiConfig::default_initial_retry_backoff_sec(),
                retry_backoff_multiplier: TeeProverApiConfig::default_retry_backoff_multiplier(),
                max_backoff_sec: TeeProverApiConfig::default_max_backoff_sec(),
            },
        };
        let tee_prover = TeeProver {
            config: tee_prover_config,
            api_client: TeeApiClient::new(Url::parse("http://mock").unwrap()),
        };
        let private_key: K256PrivateKey = signing_key.into();
        let expected_address = "0x627306090abaB3A6e1400e9345bC60c78a8BEf57"
            .parse()
            .unwrap();
        assert_eq!(private_key.address(), expected_address);

        // Generate a random root hash, create a message from the hash, and sign the message using
        // the secret key
        let random_root_hash = H256::random();
        let signature = tee_prover.sign_message(&random_root_hash).unwrap();

        // Recover the signer's Ethereum address from the signature and the message, and verify it
        // matches the expected address
        let recovered_pubkey = recover(&signature, &random_root_hash).unwrap();
        let proof_address = public_to_address(&recovered_pubkey);

        assert_eq!(proof_address, expected_address);
    }
}
