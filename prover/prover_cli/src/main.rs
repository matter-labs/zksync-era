use prover_cli::cli;

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_module("zksync_db_connection", log::LevelFilter::Off)
        .filter_module("sqlx", log::LevelFilter::Off)
        .filter_level(log::LevelFilter::Debug)
        .init();

    match cli::start().await {
        Ok(_) => {}
        Err(err) => {
            log::error!("{err:?}");
            std::process::exit(1);
        }
    }
}
