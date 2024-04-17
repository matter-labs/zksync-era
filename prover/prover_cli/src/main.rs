use prover_cli::cli;

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_module("zksync_db_connection::connection_pool", log::LevelFilter::Off)
        .filter_module("sqlx::query", log::LevelFilter::Off)
        .filter_level(log::LevelFilter::Debug)
        .init();

    cli::start().await.unwrap();
}
