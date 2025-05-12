use anyhow::Context;
use clap::Args as ClapArgs;
use dialoguer::{theme::ColorfulTheme, Input};
use zksync_prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_types::{L1BatchId, L1BatchNumber, L2ChainId};

use crate::cli::ProverCLIConfig;

#[derive(ClapArgs)]
pub struct Args {
    /// Delete data from all batches
    #[clap(
        short,
        long,
        required_unless_present = "batch",
        conflicts_with = "batch",
        default_value_t = false
    )]
    all: bool,
    /// Batch number to delete
    #[clap(short, long, required_unless_present = "all", conflicts_with = "all", default_value_t = L1BatchNumber(0))]
    batch: L1BatchNumber,
}

pub async fn run(args: Args, config: ProverCLIConfig) -> anyhow::Result<()> {
    let confirmation = Input::<String>::with_theme(&ColorfulTheme::default())
        .with_prompt("Are you sure you want to delete the data?")
        .default("no".to_owned())
        .interact_text()?;

    if confirmation != "yes" {
        println!("Aborted");
        return Ok(());
    }

    let prover_connection_pool = ConnectionPool::<Prover>::singleton(config.db_url)
        .build()
        .await
        .context("failed to build a prover_connection_pool")?;
    let conn = prover_connection_pool.connection().await.unwrap();

    if args.all {
        delete_prover_db(conn).await?;
    } else {
        delete_batch_data(conn, args.batch).await?;
    }

    Ok(())
}

async fn delete_prover_db(mut conn: Connection<'_, Prover>) -> anyhow::Result<()> {
    conn.fri_prover_jobs_dal()
        .delete()
        .await
        .context("failed to delete prover jobs")?;
    conn.fri_protocol_versions_dal()
        .delete()
        .await
        .context("failed to delete protocol versions")?;
    conn.fri_proof_compressor_dal()
        .delete()
        .await
        .context("failed to delete proof compressor")?;
    conn.fri_witness_generator_dal()
        .delete()
        .await
        .context("failed to delete witness generator")?;
    Ok(())
}

async fn delete_batch_data(
    mut conn: Connection<'_, Prover>,
    block_number: L1BatchNumber,
) -> anyhow::Result<()> {
    let batch_id = L1BatchId::new(L2ChainId::zero(), block_number);
    conn.fri_proof_compressor_dal()
        .delete_batch_data(batch_id)
        .await
        .context("failed to delete proof compressor data")?;
    conn.fri_prover_jobs_dal()
        .delete_batch_data(batch_id)
        .await
        .context("failed to delete prover jobs data")?;
    conn.fri_witness_generator_dal()
        .delete_batch_data(batch_id)
        .await
        .context("failed to delete witness generator data")?;
    Ok(())
}
