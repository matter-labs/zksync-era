use anyhow::Context as _;
use zksync_basic_types::L1BatchNumber;
use zksync_config::{ContractsConfig, EthConfig, PostgresConfig};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_env_config::FromEnv;
use zksync_eth_client::{clients::QueryClient, CallFunctionArgs, EthInterface};

// fn pretty_print_l1_status(
//     total_batches_committed: Vec<Token>,
//     total_batches_verified: Vec<Token>,
//     first_state_keeper_l1_batch: L1BatchNumber,
//     last_state_keeper_l1_batch: L1BatchNumber
// ) {
//     println!("HOLIS");
// }

pub(crate) async fn run() -> anyhow::Result<()> {
    let contracts_config = ContractsConfig::from_env()?;
    let eth_config = EthConfig::from_env()?;
    let query_client = QueryClient::new(&eth_config.web3_url).unwrap();

    let args_for_total_batches_committed: zksync_eth_client::ContractCall =
        CallFunctionArgs::new("getTotalBatchesCommitted", ()).for_contract(
            contracts_config.diamond_proxy_addr,
            zksync_contracts::zksync_contract(),
        );
    let total_batches_committed = query_client
        .call_contract_function(args_for_total_batches_committed)
        .await?;

    let args_for_total_batches_verified: zksync_eth_client::ContractCall =
        CallFunctionArgs::new("getTotalBatchesVerified", ()).for_contract(
            contracts_config.diamond_proxy_addr,
            zksync_contracts::zksync_contract(),
        );
    let total_batches_verified = query_client
        .call_contract_function(args_for_total_batches_verified)
        .await?;

    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;

    let connection_pool = ConnectionPool::<Core>::builder(
        postgres_config.replica_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await?;

    let mut conn = connection_pool.connection().await?;

    // Using unwrap() safely as there will always be at least one block.
    let first_state_keeper_l1_batch = conn
        .blocks_dal()
        .get_earliest_l1_batch_number()
        .await
        .unwrap()
        .unwrap();
    let last_state_keeper_l1_batch = conn
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        .unwrap();

    // pretty_print_l1_status(total_batches_committed, total_batches_verified, first_state_keeper_l1_batch, last_state_keeper_l1_batch);

    Ok(())
}
