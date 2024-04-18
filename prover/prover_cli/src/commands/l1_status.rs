use anyhow::Context as _;
use prover_dal::{Prover, ProverDal};
use zksync_basic_types::{
    ethabi::{Contract, Token},
    protocol_version::{L1VerifierConfig, VerifierParams},
    web3::contract::tokens::Detokenize,
    Address, L1BatchNumber, H256, U256,
};
use zksync_config::{ContractsConfig, EthConfig, PostgresConfig};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_env_config::FromEnv;
use zksync_eth_client::{clients::QueryClient, CallFunctionArgs, EthInterface};
use zksync_types::web3::contract;

pub(crate) async fn run() -> anyhow::Result<()> {
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;
    let contracts_config = ContractsConfig::from_env().context("ContractsConfig::from_env()")?;
    let eth_config = EthConfig::from_env()?;
    let query_client = QueryClient::new(&eth_config.web3_url).unwrap();

    let total_batches_committed_tokens = contract_call(
        "getTotalBatchesCommitted",
        contracts_config.diamond_proxy_addr,
        zksync_contracts::zksync_contract(),
        &query_client,
    )
    .await?;

    let mut total_batches_committed: U256 = U256::zero();
    if let Some(Token::Uint(value)) = total_batches_committed_tokens.first() {
        total_batches_committed = value.into();
    }

    let total_batches_verified_tokens = contract_call(
        "getTotalBatchesVerified",
        contracts_config.diamond_proxy_addr,
        zksync_contracts::zksync_contract(),
        &query_client,
    )
    .await?;

    let mut total_batches_verified: U256 = U256::zero();
    if let Some(Token::Uint(value)) = total_batches_verified_tokens.first() {
        total_batches_verified = value.into();
    }

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

    let node_verification_key_hash_tokens = contract_call(
        "verificationKeyHash",
        contracts_config.verifier_addr,
        zksync_contracts::verifier_contract(),
        &query_client,
    )
    .await?;

    let node_verifier_params_tokens = contract_call(
        "getVerifierParams",
        contracts_config.diamond_proxy_addr,
        zksync_contracts::zksync_contract(),
        &query_client,
    )
    .await?;

    let node_l1_verifier_config = L1VerifierConfig {
        params: VerifierParams::from_tokens(node_verifier_params_tokens)?,
        recursion_scheduler_level_vk_hash: H256::from_tokens(node_verification_key_hash_tokens)?,
    };

    let prover_connection_pool = ConnectionPool::<Prover>::builder(
        postgres_config.prover_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a prover_connection_pool")?;

    let mut conn = prover_connection_pool.connection().await.unwrap();

    let db_l1_verifier_config = conn
        .fri_protocol_versions_dal()
        .get_l1_verifier_config()
        .await;

    pretty_print_l1_verifier_config(node_l1_verifier_config, db_l1_verifier_config);

    Ok(())
}

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

fn print_hash_comparison(name: &str, contract_hash: H256, db_hash: H256) {
    if contract_hash != db_hash {
        println!("{} hash in DB differs from the one in contract.", name);
        println!("Contract hash: {}", contract_hash);
        println!("DB hash: {}", db_hash);
    } else {
        println!("{} hash matches: {}", name, contract_hash);
    }
}

fn pretty_print_l1_verifier_config(
    node_l1_verifier_config: L1VerifierConfig,
    db_l1_verifier_config: L1VerifierConfig,
) {
    print_hash_comparison(
        "Verifier key",
        node_l1_verifier_config.recursion_scheduler_level_vk_hash,
        db_l1_verifier_config.recursion_scheduler_level_vk_hash,
    );
    print_hash_comparison(
        "Verification node",
        node_l1_verifier_config.params.recursion_node_level_vk_hash,
        db_l1_verifier_config.params.recursion_node_level_vk_hash,
    );
    print_hash_comparison(
        "Verification leaf",
        node_l1_verifier_config.params.recursion_leaf_level_vk_hash,
        db_l1_verifier_config.params.recursion_leaf_level_vk_hash,
    );
    print_hash_comparison(
        "Verification circuits",
        node_l1_verifier_config
            .params
            .recursion_circuits_set_vks_hash,
        db_l1_verifier_config.params.recursion_circuits_set_vks_hash,
    );
}

async fn contract_call(
    method: &str,
    address: Address,
    contract: Contract,
    query_client: &QueryClient,
) -> anyhow::Result<Vec<Token>> {
    let args_for_total_batches_committed: zksync_eth_client::ContractCall =
        CallFunctionArgs::new(method, ()).for_contract(address, contract);
    query_client
        .call_contract_function(args_for_total_batches_committed)
        .await
        .context("call_contract_function()")
}
