use clap::Parser;
use prover_cli::cli::ProverCLI;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .init();

    let prover = ProverCLI::parse();

    match prover.start().await {
        Ok(_) => {}
        Err(err) => {
            tracing::error!("{err:?}");
            std::process::exit(1);
        }
    }
}
