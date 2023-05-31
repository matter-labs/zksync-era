extern crate core;

mod checker;
mod config;
mod helpers;

use crate::config::CheckerConfig;
use checker::Checker;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = vlog::init();

    vlog::info!("Started the Cross Node Checker");

    let config = CheckerConfig::from_env();
    let cross_node_checker = Checker::new(config);

    match cross_node_checker.run().await {
        Ok(()) => {
            vlog::info!("Cross node checker finished with no error");
        }
        Err(err) => {
            vlog::error!("Unexpected error in the checker: {}", err);
        }
    }

    Ok(())
}
