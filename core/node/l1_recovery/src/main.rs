#![feature(array_chunks)]
#![feature(iter_next_chunk)]
mod processor;

mod storage;

mod l1_fetcher;

mod utils;

use ::anyhow::Result;
use tracing_subscriber::{filter::LevelFilter, EnvFilter};

fn start_logger(default_level: LevelFilter) {
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter
            .add_directive("hyper=off".parse().unwrap())
            .add_directive("ethers=off".parse().unwrap()),
        _ => EnvFilter::default()
            .add_directive(default_level.into())
            .add_directive("hyper=off".parse().unwrap())
            .add_directive("ethers=off".parse().unwrap())
            .add_directive("zksync_storage=off".parse().unwrap()),
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<()> {
    start_logger(LevelFilter::INFO);

    Ok(())
}
