use anyhow::Context;
use zksync_basic_types::{protocol_version::L1VerifierConfig, L1BatchNumber, H256, U256};
use zksync_config::{
    configs::{DatabaseSecrets, L1Secrets},
    ContractsConfig, PostgresConfig,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::{
    clients::{Client, L1},
    CallFunctionArgs, ContractCallError,
};
use zksync_prover_dal::{Prover, ProverDal};

use crate::helper::{self, FromEnvButReallyJustExplode};

const FFLONK_VERIFIER_TYPE: i32 = 0;

pub(crate) async fn run() -> anyhow::Result<()> {
    println!(" ====== L1 Status ====== ");
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env")?;
    let contracts_config = ContractsConfig::from_env().context("ContractsConfig::from_env()")?;

    let database_secrets = DatabaseSecrets::from_env().context("DatabaseSecrets::from_env()")?;
    let l1_secrets = L1Secrets::from_env().context("L1Secrets::from_env()")?;
    let query_client =
        Client::<L1>::http(l1_secrets.l1_rpc_url.context("l1_secrets.l1_rpc_url")?)?.build();

    let total_batches_committed: U256 = CallFunctionArgs::new("getTotalBatchesCommitted", ())
        .for_contract(
            contracts_config.l1.diamond_proxy_addr,
            &helper::hyperchain_contract(),
        )
        .call(&query_client)
        .await?;

    let total_batches_verified: U256 = CallFunctionArgs::new("getTotalBatchesVerified", ())
        .for_contract(
            contracts_config.l1.diamond_proxy_addr,
            &helper::hyperchain_contract(),
        )
        .call(&query_client)
        .await?;

    let connection_pool = ConnectionPool::<Core>::builder(
        database_secrets
            .replica_url()
            .context("postgres_config.replica_url()")?,
        postgres_config
            .max_connections()
            .context("postgres_config.max_connections()")?,
    )
    .build()
    .await
    .context("ConnectionPoolBuilder::build()")?;

    let mut conn = connection_pool.connection().await?;

    // Using unwrap() safely as there will always be at least one block.
    let first_state_keeper_l1_batch = conn
        .blocks_dal()
        .get_earliest_l1_batch_number()
        .await?
        .unwrap();
    let last_state_keeper_l1_batch = conn
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await?
        .unwrap();

    pretty_print_l1_status(
        total_batches_committed,
        total_batches_verified,
        first_state_keeper_l1_batch,
        last_state_keeper_l1_batch,
    );

    let node_verification_key_hash: H256 = CallFunctionArgs::new("verificationKeyHash", ())
        .for_contract(
            contracts_config.l1.verifier_addr,
            &helper::verifier_contract(),
        )
        .call(&query_client)
        .await?;

    // We are getting function separately to get the second function with the same name, but
    // overriden one
    let contract = helper::verifier_contract();
    let function = contract
        .functions_by_name("verificationKeyHash")
        .map_err(ContractCallError::Function)?
        .get(1);

    let fflonk_verification_key_hash: Option<H256> = if let Some(function) = function {
        CallFunctionArgs::new("verificationKeyHash", U256::from(FFLONK_VERIFIER_TYPE))
            .for_contract(
                contracts_config.l1.verifier_addr,
                &helper::verifier_contract(),
            )
            .call_with_function(&query_client, function.clone())
            .await
            .ok()
    } else {
        None
    };

    let node_l1_verifier_config = L1VerifierConfig {
        snark_wrapper_vk_hash: node_verification_key_hash,
        fflonk_snark_wrapper_vk_hash: fflonk_verification_key_hash,
    };

    let prover_connection_pool = ConnectionPool::<Prover>::builder(
        database_secrets
            .prover_url()
            .context("postgres_config.replica_url()")?,
        postgres_config
            .max_connections()
            .context("postgres_config.max_connections()")?,
    )
    .build()
    .await
    .context("ConnectionPoolBuilder::build()")?;

    let mut conn = prover_connection_pool.connection().await.unwrap();

    let db_l1_verifier_config = conn
        .fri_protocol_versions_dal()
        .get_l1_verifier_config()
        .await?;

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
    println!(" ----------------------- ");
    if contract_hash != db_hash {
        println!("{name} hash in DB differs from the one in contract.");
        println!("Contract hash: {contract_hash:?}");
        println!("DB hash: {db_hash:?}");
    } else {
        println!("{name} hash matches: {contract_hash:}");
    }
}

fn pretty_print_l1_verifier_config(
    node_l1_verifier_config: L1VerifierConfig,
    db_l1_verifier_config: L1VerifierConfig,
) {
    print_hash_comparison(
        "Verifier key",
        node_l1_verifier_config.snark_wrapper_vk_hash,
        db_l1_verifier_config.snark_wrapper_vk_hash,
    );
}
