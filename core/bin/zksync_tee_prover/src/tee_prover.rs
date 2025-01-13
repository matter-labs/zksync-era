use std::fmt;

use secp256k1::{PublicKey, Secp256k1};
use zksync_basic_types::{web3, L1BatchNumber, H256};
use zksync_crypto_primitives::K256PrivateKey;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};
use zksync_prover_interface::inputs::TeeVerifierInput;
use zksync_tee_verifier::Verify;

use crate::{
    api_client::TeeApiClient, config::TeeProverConfig, error::TeeProverError, metrics::METRICS,
};

type Signature = [u8; 65];

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
pub trait SignatureSerialization {
    fn to_bytes(&self) -> Signature;
}

impl SignatureSerialization for web3::Signature {
    fn to_bytes(&self) -> Signature {
        let mut bytes = [0u8; 65];
        bytes[..32].copy_from_slice(self.r.as_bytes());
        bytes[32..64].copy_from_slice(self.s.as_bytes());
        bytes[64] = self.v as u8;
        bytes
    }
}

impl TeeProver {
    /// Signs the message in Ethereum-compatible format for on-chain verification.
    pub fn sign_message(&self, message: &H256) -> Result<Signature, TeeProverError> {
        let secret_bytes = self.config.signing_key.secret_bytes();
        let private_key = K256PrivateKey::from_bytes(secret_bytes.into())
            .map_err(|e| TeeProverError::Verification(e.into()))?;
        let signature = private_key.sign_web3(message, None).to_bytes();
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
        match self.api_client.get_job(self.config.tee_type).await {
            Ok(Some(job)) => {
                let (signature, batch_number, root_hash) = self.verify(job)?;
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

    use anyhow::Result;
    use secp256k1::{
        ecdsa::{RecoverableSignature, RecoveryId},
        Message, SecretKey, SECP256K1,
    };
    use url::Url;
    use zksync_basic_types::{self, tee_types::TeeType, web3::keccak256, Address, H512};

    use super::*;

    type Public = H512;

    /// Recovers the public key from the signature for the message
    fn recover(signature: &Signature, message: &Message) -> Result<Public> {
        let rsig = RecoverableSignature::from_compact(
            &signature[0..64],
            RecoveryId::from_i32(signature[64] as i32 - 27)?,
        )?;
        let pubkey = &SECP256K1.recover_ecdsa(&Message::from_slice(&message[..])?, &rsig)?;
        let serialized = pubkey.serialize_uncompressed();
        let mut public = Public::default();
        public.as_bytes_mut().copy_from_slice(&serialized[1..65]);
        Ok(public)
    }

    /// Convert public key into the address
    fn public_to_address(public: &Public) -> Address {
        let hash = keccak256(public.as_bytes());
        let mut result = Address::zero();
        result.as_bytes_mut().copy_from_slice(&hash[12..]);
        result
    }

    #[test]
    fn test_recover() {
        let signing_key = SecretKey::from_slice(
            &hex::decode("c87509a1c067bbde78beb793e6fa76530b6382a4c0241e5e4a9ec0a0f44dc0d3")
                .unwrap(),
        )
        .unwrap();
        let tee_prover_config = TeeProverConfig {
            signing_key,
            attestation_quote_file_path: PathBuf::from("/tmp/mock"),
            tee_type: TeeType::Sgx,
            api_url: Url::parse("http://mock").unwrap(),
            max_retries: TeeProverConfig::default_max_retries(),
            initial_retry_backoff_sec: TeeProverConfig::default_initial_retry_backoff_sec(),
            retry_backoff_multiplier: TeeProverConfig::default_retry_backoff_multiplier(),
            max_backoff_sec: TeeProverConfig::default_max_backoff_sec(),
        };
        let tee_prover = TeeProver {
            config: tee_prover_config,
            api_client: TeeApiClient::new(Url::parse("http://mock").unwrap()),
        };
        let private_key = K256PrivateKey::from_bytes(signing_key.secret_bytes().into()).unwrap();
        let expected_address =
            Address::from_slice(&hex::decode("627306090abaB3A6e1400e9345bC60c78a8BEf57").unwrap());
        assert!(private_key.address() == expected_address);

        // Generate a random root hash, create a message from the hash, and sign the message using
        // the secret key
        let random_root_hash = H256::random();
        let signature = tee_prover.sign_message(&random_root_hash).unwrap();

        // Recover the signer's Ethereum address from the signature and the message, and verify it
        // matches the expected address
        let message = Message::from_slice(random_root_hash.as_bytes()).unwrap();
        let recovered_pubkey = recover(&signature, &message).unwrap();
        let proof_address = public_to_address(&recovered_pubkey);

        assert_eq!(proof_address, expected_address);
    }
}
