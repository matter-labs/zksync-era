use std::{path::PathBuf, time::Duration};

use anyhow::Context as _;
use k256::ecdsa::{signature::Signer, Signature, SigningKey, VerifyingKey};
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};
use url::Url;
use zksync_basic_types::H256;
use zksync_config::configs::ObservabilityConfig;
use zksync_env_config::FromEnv;
use zksync_node_framework::{
    implementations::layers::sigint::SigintHandlerLayer,
    service::{ServiceContext, StopReceiver, ZkStackServiceBuilder},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};
use zksync_prover_interface::{
    api::{
        GenericProofGenerationDataResponse, RegisterTeeAttestationRequest,
        RegisterTeeAttestationResponse, SubmitProofResponse, SubmitTeeProofRequest,
        TeeProofGenerationDataRequest,
    },
    outputs::L1BatchTeeProofForL1,
};
use zksync_tee_verifier::TeeVerifierInput;
use zksync_types::{tee_types::TeeType, L1BatchNumber};

// TODO(patrick) refactor; this is already defined elsewhere but it's private
pub type TeeProofGenerationDataResponse = GenericProofGenerationDataResponse<TeeVerifierInput>;

struct ApiClient {
    api_base_url: Url,
    http_client: Client,
}

impl ApiClient {
    pub fn new(api_base_url: Url, http_client: Client) -> Self {
        ApiClient {
            api_base_url,
            http_client,
        }
    }

    async fn send_http_request<Req, Resp>(
        &self,
        request: Req,
        endpoint: Url,
    ) -> Result<Resp, reqwest::Error>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        tracing::info!("Sending request to {}", endpoint);

        self.http_client
            .post(endpoint)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<Resp>()
            .await
    }

    pub async fn register_attestation(
        &self,
        attestation_quote_bytes: Vec<u8>,
        signing_key: &SigningKey,
    ) -> anyhow::Result<()> {
        let endpoint = self.api_base_url.join("/tee/register_attestation")?;
        let request = RegisterTeeAttestationRequest {
            attestation: attestation_quote_bytes,
            pubkey: signing_key.verifying_key().to_sec1_bytes().into(),
        };
        let response = self
            .send_http_request::<RegisterTeeAttestationRequest, RegisterTeeAttestationResponse>(
                request,
                endpoint.clone(),
            )
            .await?;
        match response {
            RegisterTeeAttestationResponse::Success => {
                tracing::info!("Attestation quote was successfully registered");
                Ok(())
            }
            RegisterTeeAttestationResponse::Error(error) => {
                let err_msg = format!("Registering attestation quote failed: {}", error);
                tracing::error!(err_msg);
                Err(anyhow::anyhow!(err_msg))
            }
        }
    }

    pub async fn get_job(&self) -> anyhow::Result<Option<Box<TeeVerifierInput>>> {
        let endpoint = self.api_base_url.join("/tee/proof_inputs")?;
        let request = TeeProofGenerationDataRequest {};
        let response = self
            .send_http_request::<TeeProofGenerationDataRequest, TeeProofGenerationDataResponse>(
                request,
                endpoint.clone(),
            )
            .await?;
        match response {
            TeeProofGenerationDataResponse::Success(tvi) => Ok(tvi),
            TeeProofGenerationDataResponse::Error(err) => {
                let err_msg = format!("Failed to get proof gen data: {:?}", err);
                tracing::error!(err_msg);
                Err(anyhow::anyhow!(err_msg))
            }
        }
    }

    pub async fn submit_proof(
        &self,
        batch_number: L1BatchNumber,
        signature: Signature,
        pubkey: &VerifyingKey,
        root_hash: H256,
        tee_type: TeeType,
    ) -> anyhow::Result<()> {
        let submit_proof_endpoint = self.api_base_url.join("/tee/submit_proofs")?;
        let mut endpoint = submit_proof_endpoint.clone();
        endpoint
            .path_segments_mut()
            .unwrap()
            .push(batch_number.to_string().as_str());
        let request = SubmitTeeProofRequest(Box::new(L1BatchTeeProofForL1 {
            signature: signature.to_vec(),
            pubkey: pubkey.to_sec1_bytes().into(),
            proof: root_hash.as_bytes().into(),
            tee_type,
        }));
        let response = self
            .send_http_request::<SubmitTeeProofRequest, SubmitProofResponse>(
                request,
                endpoint.clone(),
            )
            .await?;
        match response {
            SubmitProofResponse::Success => {
                tracing::info!("Proof was successfully submitted");
                Ok(())
            }
            SubmitProofResponse::Error(error) => {
                let err_msg = format!("Submission of the proof failed: {}", error);
                tracing::error!(err_msg);
                Err(anyhow::anyhow!(err_msg))
            }
        }
    }
}

struct TeeProver {
    signing_key: SigningKey,
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
                let signature = self.signing_key.try_sign(root_hash_bytes)?;
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
            let pubkey = self.signing_key.clone();
            self.api_client
                .submit_proof(
                    batch_number,
                    signature,
                    pubkey.verifying_key(),
                    root_hash,
                    self.tee_type,
                )
                .await?;
        }
    }
}

struct TeeProverLayer {
    api_url: Url,
    signing_key: SigningKey,
    attestation_quote_bytes: Vec<u8>,
    tee_type: TeeType,
}

impl TeeProverLayer {
    pub fn new(
        api_url: Url,
        signing_key: SigningKey,
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

struct TeeProverConfig {
    signing_key: SigningKey,
    attestation_quote_file_path: PathBuf,
    tee_type: TeeType,
    api_url: Url,
}

impl FromEnv for TeeProverConfig {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            signing_key: std::env::var("TEE_SIGNING_KEY")?.parse()?,
            attestation_quote_file_path: std::env::var("TEE_QUOTE_FILE")?.parse()?,
            tee_type: std::env::var("TEE_TYPE")?.parse()?,
            api_url: std::env::var("TEE_API_URL")?.parse()?,
        })
    }
}

/// This application is a TEE verifier (a.k.a. a prover, or worker) that interacts with three
/// endpoints of the TEE prover interface API:
/// 1. `/tee/proof_inputs` - Fetches input data about a batch for the TEE verifier to process.
/// 2. `/tee/submit_proofs/<l1_batch_number>` - Submits the TEE proof, which is a signature of the
///    root hash.
/// 3. `/tee/register_attestation` - Registers the TEE attestation that binds a key used to sign a
///    root hash to the enclave where the signing process occurred. This effectively proves that the
///    signature was produced in a trusted execution environment.
///
/// Conceptually it works as follows:
/// 1. Get the TEE_SIGNING_KEY private key from the environment variable.
/// 2. Get the file path for the file containing the TEE quote from the TEE_QUOTE_FILE environment
///    variable.
/// 3. Register the attestation via the `/tee/register_attestation` endpoint.
/// 4. Run a loop:
///    a. Fetch the next batch data via the `/tee/proof_inputs` endpoint.
///    b. Verify the batch data.
///    c. If verification is successful, sign the root hash of the batch data with the private key.
///    d. Submit the signature (a.k.a. proof) via the `/tee/submit_proofs/<l1_batch_number>`
///       endpoint.
fn main() -> anyhow::Result<()> {
    let observability_config =
        ObservabilityConfig::from_env().context("ObservabilityConfig::from_env()")?;
    let log_format: zksync_vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;
    let mut builder = zksync_vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = observability_config.sentry_url {
        builder = builder
            .with_sentry_url(&sentry_url)
            .context("Invalid Sentry URL")?
            .with_sentry_environment(observability_config.sentry_environment);
    }
    let _guard = builder.build();

    let tee_prover_config = TeeProverConfig::from_env().context("TeeProverConfig::from_env()")?;
    let signing_key = &tee_prover_config.signing_key;
    let _verifying_key_bytes = signing_key.verifying_key().to_sec1_bytes();

    // TEST TEST
    {
        use k256::ecdsa::signature::Verifier;
        let vkey: VerifyingKey = VerifyingKey::try_from(_verifying_key_bytes.as_ref())?;
        let signature: Signature = signing_key.try_sign(&[0, 0, 0, 0])?;
        let sig_bytes = signature.to_vec();
        let signature: Signature = Signature::try_from(sig_bytes.as_ref())?;
        let _ = vkey.verify(&[0, 0, 0, 0], &signature)?;
    }
    // END TEST

    let attestation_quote_bytes = std::fs::read(tee_prover_config.attestation_quote_file_path)?;

    // let prometheus_config = PrometheusConfig::from_env().ok();
    // if let Some(prometheus_config) = prometheus_config {
    //     let exporter_config = PrometheusExporterConfig::push(
    //         prometheus_config.gateway_endpoint(),
    //         prometheus_config.push_interval(),
    //     );

    //     tracing::info!("Starting prometheus exporter with config {prometheus_config:?}");
    //     let prometheus_exporter_task = tokio::spawn(exporter_config.run(stop_receiver));
    // } else {
    //     bail!("No Prometheus configuration found");
    // }

    ZkStackServiceBuilder::new()
        .add_layer(SigintHandlerLayer)
        // .add_layer(PrometheusExporterLayer(prometheus_config?))
        .add_layer(TeeProverLayer::new(
            tee_prover_config.api_url,
            tee_prover_config.signing_key,
            attestation_quote_bytes,
            tee_prover_config.tee_type,
        ))
        .build()?
        .run()?;

    Ok(())
}
