use std::fmt;

use secp256k1::{Message, PublicKey, Secp256k1, SecretKey, SECP256K1};
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
    /// Signs the message in Ethereum-compatible format for on-chain verification.
    pub fn sign_message(sec: &SecretKey, message: Message) -> Result<[u8; 65], TeeProverError> {
        let s = SECP256K1.sign_ecdsa_recoverable(&message, sec);
        let (rec_id, data) = s.serialize_compact();

        let mut signature = [0u8; 65];
        signature[..64].copy_from_slice(&data);
        // as defined in the Ethereum Yellow Paper (Appendix F)
        // https://ethereum.github.io/yellowpaper/paper.pdf
        signature[64] = 27 + rec_id.to_i32() as u8;

        Ok(signature)
    }

    fn verify(
        &self,
        tvi: TeeVerifierInput,
    ) -> Result<([u8; 65], L1BatchNumber, H256), TeeProverError> {
        match tvi {
            TeeVerifierInput::V1(tvi) => {
                let observer = METRICS.proof_generation_time.start();
                let verification_result = tvi.verify().map_err(TeeProverError::Verification)?;
                let root_hash_bytes = verification_result.value_hash.as_bytes();
                let batch_number = verification_result.batch_number;
                let msg_to_sign = Message::from_slice(root_hash_bytes)
                    .map_err(|e| TeeProverError::Verification(e.into()))?;
                let signature = TeeProver::sign_message(&self.config.signing_key, msg_to_sign)?;
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
    use anyhow::Result;
    use secp256k1::ecdsa::{RecoverableSignature, RecoveryId};
    use sha3::{Digest, Keccak256};

    use super::*;

    /// Converts a public key into an Ethereum address by hashing the encoded public key with Keccak256.
    pub fn public_key_to_ethereum_address(public: &PublicKey) -> [u8; 20] {
        let public_key_bytes = public.serialize_uncompressed();

        // Skip the first byte (0x04) which indicates uncompressed key
        let hash: [u8; 32] = Keccak256::digest(&public_key_bytes[1..]).into();

        // Take the last 20 bytes of the hash to get the Ethereum address
        let mut address = [0u8; 20];
        address.copy_from_slice(&hash[12..]);
        address
    }

    /// Equivalent to the ecrecover precompile, ensuring that the signatures we produce off-chain
    /// can be recovered on-chain.
    pub fn recover_signer(sig: &[u8; 65], msg: &Message) -> Result<[u8; 20]> {
        let sig = RecoverableSignature::from_compact(
            &sig[0..64],
            RecoveryId::from_i32(sig[64] as i32 - 27)?,
        )?;
        let public = SECP256K1.recover_ecdsa(msg, &sig)?;
        Ok(public_key_to_ethereum_address(&public))
    }

    #[test]
    fn recover() {
        // Decode the sample secret key, generate the public key, and derive the Ethereum address
        // from the public key
        let secp = Secp256k1::new();
        let secret_key_bytes =
            hex::decode("c87509a1c067bbde78beb793e6fa76530b6382a4c0241e5e4a9ec0a0f44dc0d3")
                .unwrap();
        let secret_key = SecretKey::from_slice(&secret_key_bytes).unwrap();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let expected_address = hex::decode("627306090abaB3A6e1400e9345bC60c78a8BEf57").unwrap();
        let address = public_key_to_ethereum_address(&public_key);

        assert_eq!(address, expected_address.as_slice());

        // Generate a random root hash, create a message from the hash, and sign the message using
        // the secret key
        let root_hash = H256::random();
        let root_hash_bytes = root_hash.as_bytes();
        let msg_to_sign = Message::from_slice(root_hash_bytes).unwrap();
        let signature = TeeProver::sign_message(&secret_key, msg_to_sign).unwrap();

        // Recover the signer's Ethereum address from the signature and the message, and verify it
        // matches the expected address
        let proof_addr = recover_signer(&signature, &msg_to_sign).unwrap();

        assert_eq!(proof_addr, expected_address.as_slice());
    }
}
