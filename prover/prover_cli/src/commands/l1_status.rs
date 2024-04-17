use anyhow::Context as _;
use zksync_basic_types::{ethabi::Token, L1BatchNumber, U256};
use zksync_config::{ContractsConfig, EthConfig, PostgresConfig};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_env_config::FromEnv;
use zksync_eth_client::{clients::QueryClient, CallFunctionArgs, EthInterface};

fn pretty_print_l1_status(
    total_batches_committed: U256,
    total_batches_verified: U256,
    first_state_keeper_l1_batch: L1BatchNumber,
    last_state_keeper_l1_batch: L1BatchNumber,
) {
    println!(
        "State keeper: First batch: {}, recent batch: {}",
        first_state_keeper_l1_batch, last_state_keeper_l1_batch
    );

    println!(
        "L1 state: block verified: {}, block committed: {}",
        total_batches_verified, total_batches_committed
    );

    let eth_sender_lag = U256::from(last_state_keeper_l1_batch.0) - total_batches_committed;
    if eth_sender_lag > U256::zero() {
        println!(
            "Eth sender is {} behind. Last block committed: {}. Most recent sealed state keeper batch: {}.", 
            eth_sender_lag,
            total_batches_committed,
            last_state_keeper_l1_batch
        );
    }
}

pub(crate) async fn run() -> anyhow::Result<()> {
    let contracts_config = ContractsConfig::from_env()?;
    let eth_config = EthConfig::from_env()?;
    let query_client = QueryClient::new(&eth_config.web3_url).unwrap();

    let args_for_total_batches_committed: zksync_eth_client::ContractCall =
        CallFunctionArgs::new("getTotalBatchesCommitted", ()).for_contract(
            contracts_config.diamond_proxy_addr,
            zksync_contracts::zksync_contract(),
        );
    let total_batches_committed_tokens = query_client
        .call_contract_function(args_for_total_batches_committed)
        .await?;

    let mut total_batches_committed: U256 = U256::zero();
    if let Some(Token::Uint(value)) = total_batches_committed_tokens.first() {
        total_batches_committed = value.into();
    }

    let args_for_total_batches_verified: zksync_eth_client::ContractCall =
        CallFunctionArgs::new("getTotalBatchesVerified", ()).for_contract(
            contracts_config.diamond_proxy_addr,
            zksync_contracts::zksync_contract(),
        );

    let total_batches_verified_tokens = query_client
        .call_contract_function(args_for_total_batches_verified)
        .await?;

    let mut total_batches_verified: U256 = U256::zero();
    if let Some(Token::Uint(value)) = total_batches_verified_tokens.first() {
        total_batches_verified = value.into();
    }

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
        .context("get_earliest_l1_batch_number")?
        .unwrap();
    let last_state_keeper_l1_batch = conn
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .context("last_state_keeper_l1_batch")?
        .unwrap();

    pretty_print_l1_status(
        total_batches_committed,
        total_batches_verified,
        first_state_keeper_l1_batch,
        last_state_keeper_l1_batch,
    );

    Ok(())
}
