use super::utils::BatchData;
use crate::commands::status::utils::postgres_config;
use anyhow::{ensure, Context as _};
use clap::Args as ClapArgs;

use prover_dal::{ConnectionPool, Prover};
use zksync_types::L1BatchNumber;

#[derive(ClapArgs)]
pub struct Args {
    #[clap(short = 'n', num_args = 1..)]
    batches: Vec<L1BatchNumber>,
    #[clap(short, long, default_value("false"))]
    verbose: bool,
}

pub(crate) async fn run(args: Args) -> anyhow::Result<()> {
    ensure!(
        !args.batches.is_empty(),
        "At least one batch number should be provided"
    );

    let batches_data = get_batches_data(args.batches).await?;

    for batch_data in batches_data {
        println!("{batch_data:?}");
    }

    Ok(())
}

async fn get_batches_data(_batches: Vec<L1BatchNumber>) -> anyhow::Result<Vec<BatchData>> {
    let config = postgres_config()?;

    let prover_connection_pool =
        ConnectionPool::<Prover>::builder(config.prover_url()?, config.max_connections()?)
            .build()
            .await
            .context("failed to build a prover_connection_pool")?;

    let _conn = prover_connection_pool.connection().await.unwrap();

    // Queries here...

    Ok(vec![BatchData::default()])
}
