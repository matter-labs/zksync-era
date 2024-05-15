use anyhow::Context as _;
use clap::Parser;
use futures_util::StreamExt;
use google_cloud_pubsub::client::{Client, ClientConfig};
use zksync_core::temp_config_store::decode_yaml_repr;
use zksync_object_store::StoredObject;
use zksync_tee_verifier::TeeVerifierInput;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version, about = "zkSync operator node", long_about = None)]
struct Cli {
    /// Path to the yaml config. If set, it will be used instead of env vars.
    #[arg(long)]
    config_path: Option<std::path::PathBuf>,
    /// Path to the yaml with secrets. If set, it will be used instead of env vars.
    #[arg(long)]
    secrets_path: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();

    let configs = match opt.config_path {
        None => panic!(),
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&yaml)
                .context("failed decoding general YAML config")?
        }
    };

    let observability_config = configs
        .observability
        .clone()
        .context("observability config")?;

    let log_format: vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(log_directives) = observability_config.log_directives {
        builder = builder.with_log_directives(log_directives);
    }

    let _guard = builder.build();

    let pubsub_config = ClientConfig::default().with_auth().await?;
    let client = Client::new(pubsub_config).await?;

    let subscription_name = "sgx-prover-inputs";
    let subscription = client.subscription(&subscription_name);
    if !subscription.exists(None).await? {
        return Err(anyhow::anyhow!("subscription missing"));
    }

    let mut iter: google_cloud_pubsub::subscription::MessageStream =
        subscription.subscribe(None).await?;

    while let Some(message) = iter.next().await {
        let _ = message.ack().await?;

        tracing::debug!("Received input. Generating proof ...");

        if message.message.data.len() == 0 {
            tracing::info!("empty data field");
            continue;
        }

        let input =
            TeeVerifierInput::deserialize(message.message.data).expect("Deserialization error");
        let l1_batch_nr = input.l1_batch_nr();
        let result = input.verify();
        if result.is_ok() {
            tracing::info!("Successfully proved a batch #{:?}!", l1_batch_nr);
        }
    }

    Ok(())
}
