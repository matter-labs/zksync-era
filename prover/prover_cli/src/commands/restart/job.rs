use anyhow::Context;
use clap::Args as ClapArgs;
use prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_config::PostgresConfig;
use zksync_env_config::FromEnv;

#[derive(ClapArgs)]
pub struct Args {
    id: u32,
}

pub async fn run(args: Args) -> anyhow::Result<()> {
    let config = PostgresConfig::from_env()?;
    let connection_pool =
        ConnectionPool::<Prover>::singleton(config.master_url().context("master_url()")?)
            .build()
            .await?;
    let mut conn = connection_pool.connection().await?;
    restart_prover_job(args.id, &mut conn).await;
    Ok(())
}

async fn restart_prover_job(id: u32, conn: &mut Connection<'_, Prover>) {
    conn.fri_prover_jobs_dal().update_status(id, "queued").await;
}
