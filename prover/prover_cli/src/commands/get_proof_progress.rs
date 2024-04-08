use anyhow::Context as _;
use prover_dal::{ConnectionPool, Prover, ProverDal};
use tokio::{
    sync::{oneshot, watch::Receiver},
    task::JoinHandle,
};
use zksync_config::configs::{
    fri_prover_group::FriProverGroupConfig, FriProverConfig, ObservabilityConfig, PostgresConfig,
};
use zksync_env_config::{
    object_store::{ProverObjectStoreConfig, PublicObjectStoreConfig},
    FromEnv,
};

pub(crate) async fn run() -> eyre::Result<()> {
    log::info!("Proof Progress");

    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;
    let pool = ConnectionPool::singleton(postgres_config.prover_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;

    Ok(())
}
