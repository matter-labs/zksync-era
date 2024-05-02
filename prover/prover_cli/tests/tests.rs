use prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_config::PostgresConfig;
use zksync_env_config::FromEnv;
use zksync_types::L1BatchNumber;

#[tokio::test]
async fn prover_cli_test() {
    let postgres_config = PostgresConfig::from_env().unwrap();
    let connection_pool = ConnectionPool::<Prover>::builder(
        postgres_config.test_prover_url.clone().unwrap().as_str(),
        postgres_config.max_connections().unwrap(),
    )
    .build()
    .await
    .unwrap();
    let mut connection = connection_pool.connection().await.unwrap();
    let db_url = connection_pool.database_url();

    println!("{db_url:?}");
    println!(
        "{:?}",
        connection
            .fri_witness_generator_dal()
            .get_basic_witness_generator_job_for_batch(L1BatchNumber(0))
            .await
    );
}
