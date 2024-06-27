use anyhow::Context as _;
use config::TeeProverConfig;
use k256::ecdsa::{signature::Signer, Signature, VerifyingKey};
use tee_prover::TeeProverLayer;
use zksync_config::configs::ObservabilityConfig;
use zksync_env_config::FromEnv;
use zksync_node_framework::{
    implementations::layers::sigint::SigintHandlerLayer, service::ZkStackServiceBuilder,
};

mod api_client;
mod config;
mod tee_prover;

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
        vkey.verify(&[0, 0, 0, 0], &signature)?;
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
