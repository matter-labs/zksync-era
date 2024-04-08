use anyhow::Context as _;
use prover_dal::Prover;
use zksync_config::PostgresConfig;
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_env_config::FromEnv;

pub(crate) async fn run() -> anyhow::Result<()> {
    log::info!("Proof Progress");

    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;

    println!("{:?}", postgres_config);

    let pool = ConnectionPool::<Prover>::singleton(postgres_config.prover_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;

    // let asd = sqlx::query_as!(
    //     StorageL1BatchHeader,
    //     r#"
    //     SELECT
    //         id
    //     FROM
    //         prover_jobs_fri
    //     "#,
    // )
    // .fetch_all(pool)
    // .await?;

    Ok(())
}
