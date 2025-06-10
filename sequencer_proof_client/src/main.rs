use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::path::Path;
use zkos_wrapper::{serialize_to_file, SnarkWrapperProof};
use zksync_sequencer_proof_client::SequencerProofClient;
use zksync_types::L2BlockNumber;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Sequencer URL to submit proofs to
    #[arg(short, long, global = true, value_name = "URL")]
    url: Option<String>,

    /// Activate verbose logging (`-v`, `-vv`, ...)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    /// Regular `::parse()`, but checks that the `--url` argument is provided & initializes tracing.
    fn init() -> Result<Self> {
        let cli = Cli::parse();
        if cli.url.is_none() {
            return Err(anyhow!("The --url <URL> argument is required. It can be placed anywhere on the command line."));
        }
        init_tracing(cli.verbose);
        Ok(cli)
    }

    /// Returns the sequencer URL, which is required for all commands. To be called only after `Cli::init()`.
    fn sequencer_client(&self) -> SequencerProofClient {
        SequencerProofClient::new(self.url.clone().expect("called sequencer_client() before init()"))
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Picks the next FRI proof job from the sequencer; sequencer marks job as picked (and will not give it to other clients, until the job expires)
    PickFri {},
    /// Submits block's FRI proof to sequencer
    SubmitFri {},
    /// Picks the next SNARK proof job from the sequencer; sequencer marks job as picked (and will not give it to other clients, until the job expires)
    PickSnark {},
    /// Submits block's SNARK proof to sequencer
    SubmitSnark {
        /// Block number to submit the SNARK proof for
        #[arg(short, long, value_name = "BLOCK")]
        block: u32,
        /// Path to the SNARK proof file to submit
        #[arg(short, long, value_name = "SNARK_PATH")]
        path: String,
    },
}

fn init_tracing(verbosity: u8) {
    use tracing_subscriber::{fmt, EnvFilter};
    let level = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));
    fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::init()?;

    let client = cli.sequencer_client();

    let url = client.sequencer_url();

    match cli.command {
        Commands::PickFri {} => {
            todo!();
        }
        Commands::SubmitFri {} => {
            todo!();
        }
        Commands::PickSnark {} => {
            tracing::info!("Picking next SNARK proof job from sequencer at {}", url);
            match client.pick_snark_job().await? {
                Some((proof, block)) => {
                    // TODO: would be nice to be able to specify where to save the proof file
                    let path = format!("./three_fri_proof_{block}.json");
                    tracing::info!("Picked SNARK job for block {block}, saved proof to path {path}");
                    serialize_to_file(&proof, Path::new(&path));
                }
                None => {
                    tracing::info!("No SNARK proof jobs available at the moment.");
                }
            }
        }
        Commands::SubmitSnark { block, path } => {
            tracing::info!("Submitting SNARK proof for block {block} with proof from {path} to sequencer at {}", url);
            let file = std::fs::File::open(path)?;
            let snark_wrapper: SnarkWrapperProof = serde_json::from_reader(file)?;
            client.submit_snark_proof(L2BlockNumber(block), snark_wrapper).await?;
            tracing::info!("Submitted proof for block {block} to sequencer at {}", url);
        }
    }

    Ok(())
}
