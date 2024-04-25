use anyhow::{ensure, Context as _};
use clap::Args as ClapArgs;
use prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_types::L1BatchNumber;

use super::utils::{BatchData, Task, TaskStatus};
use crate::commands::status::utils::postgres_config;

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

async fn get_batches_data(batches: Vec<L1BatchNumber>) -> anyhow::Result<Vec<BatchData>> {
    let config = postgres_config()?;

    let prover_connection_pool =
        ConnectionPool::<Prover>::builder(config.prover_url()?, config.max_connections()?)
            .build()
            .await
            .context("failed to build a prover_connection_pool")?;

    let mut conn = prover_connection_pool.connection().await.unwrap();

    let mut batches_data = Vec::new();
    for batch in batches {
        let current_batch_data = BatchData {
            basic_witness_generator: Task::BasicWitnessGenerator(
                get_proof_basic_witness_generator_status_for_batch(batch, &mut conn).await,
            ),
            compressor: Task::Compressor(
                get_proof_compression_job_status_for_batch(batch, &mut conn).await,
            ),
            ..Default::default()
        };
        batches_data.push(current_batch_data);
    }

    Ok(batches_data)
}

async fn get_proof_basic_witness_generator_status_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> TaskStatus {
    conn.fri_witness_generator_dal()
        .get_basic_witness_generator_job_for_batch(batch_number)
        .await
        .map(|job| TaskStatus::from(job.status))
        .unwrap_or(TaskStatus::Custom(
            "Basic witness generator job not found 🚫".to_owned(),
        ))
}

async fn get_proof_compression_job_status_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> TaskStatus {
    conn.fri_proof_compressor_dal()
        .get_proof_compression_job_for_batch(batch_number)
        .await
        .map(|job| TaskStatus::from(job.status))
        .unwrap_or(TaskStatus::Custom("Compressor job not found 🚫".to_owned()))
}
