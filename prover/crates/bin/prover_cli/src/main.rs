use clap::Parser;
use prover_cli::{cli::ProverCLI, config};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .init();

    config::get_envfile()
        .and_then(config::load_envfile)
        .inspect_err(|err| {
            tracing::error!("{err:?}");
            std::process::exit(1);
        })
        .unwrap();

    let prover = ProverCLI::parse();

    match prover.start().await {
        Ok(_) => {}
        Err(err) => {
            tracing::error!("{err:?}");
            std::process::exit(1);
        }
    }
}
