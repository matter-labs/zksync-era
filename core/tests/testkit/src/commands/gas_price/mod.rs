//! Gas price test is used to calculate costs of user transactions in terms of gas price.
//! It should be used as a fast benchmark tool for optimizations of out smart contracts, and
//! as a sanity check after contract refactoring.
//!
//! It is important for several reasons:
//! * Transfer cost determines maximum possible TPS of our network in larger block size limit.
//! * Cost of operations in the verify functions could stop block verification because of the block gas limit.
//! * It is useful to calculate cost of the "griefing" attack.
//! We don't take fees for deposit and full exit, but we must process them, so it is possible to spam us and force us to spend money.

use std::time::Instant;

use rand::thread_rng;
use zksync_core::genesis::ensure_genesis_state;

use crate::commands::gas_price::utils::{
    commit_cost_of_add_tokens, commit_cost_of_deploys, commit_cost_of_deposits,
    commit_cost_of_executes, commit_cost_of_n_empty_blocks, commit_cost_of_transfers,
    commit_cost_of_transfers_to_new, commit_cost_of_withdrawals,
};
use crate::commands::utils::{
    create_first_block, create_test_accounts, get_root_hash, get_test_config, TestDatabaseManager,
};
use crate::external_commands::{deploy_contracts, deploy_erc20_tokens};
use crate::tester::Tester;
use crate::types::ETHEREUM_ADDRESS;
use crate::utils::load_test_bytecode_and_calldata;

mod types;
mod utils;

pub async fn test_gas_price() {
    const APPROX_TRANSFER_COMMIT_COST: usize = 5850;
    let config = get_test_config();

    let test_db_manager = TestDatabaseManager::new().await;
    let mut storage = test_db_manager.connect_to_postgres().await;
    {
        ensure_genesis_state(&mut storage, config.clone()).await;
    }

    println!("deploying contracts");
    let deploy_timer = Instant::now();

    let root_hash = get_root_hash(&mut storage);
    let contracts = deploy_contracts(false, root_hash);

    println!(
        "contracts deployed {:#?}, {} secs",
        contracts,
        deploy_timer.elapsed().as_secs()
    );

    let (operator_account, _) = create_test_accounts(&config, &contracts);

    let mut tester = Tester::new(test_db_manager, operator_account.clone(), config.clone()).await;

    let token = contracts.test_erc20_address;

    create_first_block(&mut tester, config.clone()).await;

    let rng = &mut thread_rng();

    let base_cost =
        commit_cost_of_n_empty_blocks(&mut tester, 1, config.clone(), contracts.zk_sync).await;

    // Aggregated blocks amortization info
    let n_blocks = 20;
    {
        let config_for_aggregated_block = {
            let mut config_for_aggregated_block = config.clone();
            config_for_aggregated_block
                .eth_sender
                .sender
                .max_aggregated_blocks_to_commit = n_blocks;
            config_for_aggregated_block
                .eth_sender
                .sender
                .max_aggregated_blocks_to_execute = n_blocks;
            config_for_aggregated_block
                .eth_sender
                .sender
                .aggregated_proof_sizes = vec![n_blocks as usize];
            config_for_aggregated_block
        };
        let base_cost_n_blocks = commit_cost_of_n_empty_blocks(
            &mut tester,
            n_blocks,
            config_for_aggregated_block.clone(),
            contracts.zk_sync,
        )
        .await;
        let commit_cost_per_block = (base_cost_n_blocks.base_commit_cost
            - base_cost.base_commit_cost.clone())
            / (n_blocks - 1)
            - APPROX_TRANSFER_COMMIT_COST;
        let commit_base_cost =
            &base_cost.base_commit_cost - &commit_cost_per_block - APPROX_TRANSFER_COMMIT_COST;
        let prove_cost_per_block = (base_cost_n_blocks.base_verify_cost
            - base_cost.base_verify_cost.clone())
            / (n_blocks - 1);
        let prove_base_cost = &base_cost.base_verify_cost - &prove_cost_per_block;
        let execute_cost_per_block = (base_cost_n_blocks.base_execute_cost
            - base_cost.base_execute_cost.clone())
            / (n_blocks - 1);
        let execute_base_cost = &base_cost.base_execute_cost - &execute_cost_per_block;
        println!("Cost of block operations (base_cost, cost_per_block):");
        println!("NOTE: aggregated blocks(n) cost of tx = base_cost + cost_per_block*n");
        println!(
            "commit: ({}, {})\nprove: ({}, {})\nexecute: ({}, {})",
            commit_base_cost,
            commit_cost_per_block,
            prove_base_cost,
            prove_cost_per_block,
            execute_base_cost,
            execute_cost_per_block
        );
        println!();
    }

    commit_cost_of_deposits(
        &mut tester,
        20,
        ETHEREUM_ADDRESS,
        config.clone(),
        contracts.zk_sync,
        rng,
    )
    .await
    .report(&base_cost, "deposit ETH", true);

    commit_cost_of_deposits(
        &mut tester,
        20,
        token,
        config.clone(),
        contracts.zk_sync,
        rng,
    )
    .await
    .report(&base_cost, "deposit ERC20", true);

    commit_cost_of_transfers(
        &mut tester,
        50,
        token,
        config.clone(),
        contracts.zk_sync,
        rng,
    )
    .await
    .report(&base_cost, "transfer", false);

    commit_cost_of_transfers_to_new(
        &mut tester,
        50,
        token,
        config.clone(),
        contracts.zk_sync,
        rng,
    )
    .await
    .report(&base_cost, "transfer to new", false);

    commit_cost_of_withdrawals(
        &mut tester,
        20,
        ETHEREUM_ADDRESS,
        config.clone(),
        contracts.zk_sync,
        rng,
    )
    .await
    .report(&base_cost, "withdrawals ETH", false);

    commit_cost_of_withdrawals(
        &mut tester,
        20,
        token,
        config.clone(),
        contracts.zk_sync,
        rng,
    )
    .await
    .report(&base_cost, "withdrawals ERC20", false);

    let (bytecode, constructor_calldata, calldata) = load_test_bytecode_and_calldata();

    commit_cost_of_deploys(
        &mut tester,
        20,
        bytecode.clone(),
        constructor_calldata.clone(),
        config.clone(),
        rng,
    )
    .await
    .report(&base_cost, "deploys", false);

    commit_cost_of_executes(
        &mut tester,
        20,
        bytecode,
        constructor_calldata,
        calldata,
        config.clone(),
        rng,
    )
    .await
    .report(&base_cost, "executes", false);

    let tokens = deploy_erc20_tokens();
    commit_cost_of_add_tokens(&mut tester, tokens, config.clone())
        .await
        .report(&base_cost, "add_tokens", false);
}
