extern crate core;

use std::time::Duration;

use anyhow::{anyhow, Context};
use clap::Parser;
use prometheus_exporter::PrometheusExporterConfig;
use rand::rngs::OsRng;
use reqwest::Client;
use secp256k1::Keypair;
use tokio::sync::{oneshot, watch};
use url::Url;
use zksync_prover_config::load_general_config;
use zksync_prover_interface::api::{
    RegisterTeeAttestationRequest, RegisterTeeAttestationResponse, TeeProofGenerationDataRequest,
};
use zksync_utils::wait_for_tasks::ManagedTasks;

use crate::{
    api_data_fetcher::PeriodicApiStruct,
    attestation::{get_attestation_quote, save_attestation_user_report_data},
};

mod api_data_fetcher;
mod attestation;
mod metrics;
mod proof_gen_data_fetcher;

/// The path to the TEE API endpoint that returns the next proof generation data
pub(crate) const PROOF_GENERATION_DATA_ENDPOINT: &str = "/tee/proof_inputs";

/// The path to the API endpoint that submits the proof
pub(crate) const SUBMIT_PROOF_ENDPOINT: &str = "/tee/submit_proofs";

/// The path to the API endpoint that registers the attestation
pub(crate) const REGISTER_ATTESTATION_ENDPOINT: &str = "/tee/register_attestation";

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    /// Path of the configuration file.
    #[arg(long)]
    pub(crate) config_path: Option<std::path::PathBuf>,

    /// In simulation mode, a simulated/mocked attestation is used, read from the
    /// ATTESTATION_QUOTE_FILE environment variable file. To fetch a real attestation, the
    /// application must be run inside an enclave.
    #[arg(long)]
    simulation: bool,

    /// The URL of the TEE prover interface API.
    #[arg(long)]
    endpoint_url: String,
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
/// 1. Generate a new priv-public key pair.
/// 2. Generate an attestation quote for the public key.
/// 3. Register the attestation via the `/tee/register_attestation` endpoint.
/// 4. Run a loop:
///    a. Fetch the next batch data via the `/tee/proof_inputs` endpoint.
///    b. Verify the batch data.
///    c. If verification is successful, sign the root hash of the batch data with the private key.
///    d. Submit the signature (a.k.a. proof) via the `/tee/submit_proofs/<l1_batch_number>`
///       endpoint.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();

    let general_config = load_general_config(opt.config_path).context("general config")?;

    let observability_config = general_config
        .observability
        .context("observability config")?;

    let log_format: vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = &observability_config.sentry_url {
        builder = builder
            .with_sentry_url(sentry_url)
            .expect("Invalid Sentry URL")
            .with_sentry_environment(observability_config.sentry_environment);
    }
    let _guard = builder.build();

    let config = general_config
        .prover_tee_gateway
        .context("TEE prover gateway config missing")?;

    // Generate a new priv-public key pair

    let key_pair = Keypair::new_global(&mut OsRng);

    // Get attestation quote

    let attestation_quote_bytes = if opt.simulation {
        let attestation_quote_file = std::env::var("ATTESTATION_QUOTE_FILE").unwrap_or_default();
        std::fs::read(&attestation_quote_file).unwrap_or_default()
    } else {
        save_attestation_user_report_data(key_pair.public_key())?;
        get_attestation_quote()?
    };

    // Register attestation quote

    let http_client = Client::new();
    let attestation_request = RegisterTeeAttestationRequest {
        attestation: attestation_quote_bytes,
        pubkey: key_pair.public_key().serialize().to_vec(),
    };
    let attestation_endpoint =
        Url::parse(opt.endpoint_url.as_str())?.join(REGISTER_ATTESTATION_ENDPOINT)?;
    let tee_attestation_response = http_client
        .post(attestation_endpoint)
        .json(&attestation_request)
        .send()
        .await?
        .error_for_status()?
        .json::<RegisterTeeAttestationResponse>()
        .await?;
    match tee_attestation_response {
        RegisterTeeAttestationResponse::Success => {
            println!("Attestation quote was successfully registered")
        }
        RegisterTeeAttestationResponse::Error(error) => {
            return Err(anyhow!("Registering attestation quote failed: {}", error))
        }
    }

    // Create TEE verifier input fetcher

    let api_url = Url::parse(config.api_url.as_str())?;
    let proof_gen_data_fetcher = PeriodicApiStruct {
        api_url: api_url.join(PROOF_GENERATION_DATA_ENDPOINT)?.into(),
        poll_duration: config.api_poll_duration(),
        client: Client::new(),
        key_pair: key_pair,
        submit_proof_endpoint: api_url.join(SUBMIT_PROOF_ENDPOINT)?.into(),
    };

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    tracing::info!("Starting TEE Prover Gateway");

    let tasks = vec![
        tokio::spawn(
            PrometheusExporterConfig::pull(config.prometheus_listener_port)
                .run(stop_receiver.clone()),
        ),
        tokio::spawn(
            proof_gen_data_fetcher.run::<TeeProofGenerationDataRequest>(stop_receiver.clone()),
        ),
    ];

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        }
    }
    stop_sender.send(true).ok();
    tasks.complete(Duration::from_secs(5)).await;

    Ok(())
}
