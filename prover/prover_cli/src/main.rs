use prover_cli::cli;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .init();
    match cli::start().await {
        Ok(_) => {}
        Err(err) => {
            tracing::error!("{err:?}");
            std::process::exit(1);
        }
    }
}
