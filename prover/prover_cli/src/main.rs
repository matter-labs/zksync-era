use prover_cli::cli;

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_module("hyper", log::LevelFilter::Off)
        .filter_module("web3", log::LevelFilter::Off)
        .filter_module("zksync_db_connection", log::LevelFilter::Off)
        .filter_module("sqlx", log::LevelFilter::Off)
        .filter_module("reqwest", log::LevelFilter::Off)
        .filter_level(log::LevelFilter::Debug)
        .init();

    cli::start().await.unwrap();
}
