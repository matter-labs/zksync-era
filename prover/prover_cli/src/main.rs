use prover_cli::cli;

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    cli::start().await.unwrap();
}
