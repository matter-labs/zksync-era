use num::BigUint;
use std::time::Instant;

use zksync_core::genesis::ensure_genesis_state;

use crate::commands::utils::{
    create_first_block, create_test_accounts, get_root_hashes, get_test_config,
    perform_transactions, TestDatabaseManager,
};
use crate::external_commands::{deploy_contracts, run_upgrade_contract};
use crate::tester::Tester;
use crate::types::{BlockProcessing, ETHEREUM_ADDRESS};

pub async fn test_upgrade_contract() {
    let config = get_test_config();

    let test_db_manager = TestDatabaseManager::new().await;
    let db = test_db_manager.get_db();

    ensure_genesis_state(db.clone(), config.clone());

    println!("deploying contracts");
    let deploy_timer = Instant::now();

    let (root_hash, porter_root_hash) = get_root_hashes(db.clone());
    let contracts = deploy_contracts(false, root_hash, porter_root_hash);

    println!(
        "contracts deployed {:#?}, {} secs",
        contracts,
        deploy_timer.elapsed().as_secs()
    );

    let (operator_account, rich_account) = create_test_accounts(&config, &contracts);

    let mut tester = Tester::new(db.clone(), operator_account.clone(), config.clone());

    let token = ETHEREUM_ADDRESS;
    let fee_token = contracts.test_erc20_address;
    let deposit_amount = BigUint::from(10u32).pow(18u32);

    create_first_block(&mut tester, fee_token, config.clone()).await;

    perform_transactions(
        &mut tester,
        rich_account.clone(),
        token,
        fee_token,
        deposit_amount.clone(),
        contracts.zk_sync,
        config.clone(),
        BlockProcessing::CommitAndExecute,
    )
    .await;

    let start_upgrade = Instant::now();
    run_upgrade_contract(contracts.zk_sync, contracts.upgrade_gatekeeper);
    println!("Upgrade done in {:?}", start_upgrade.elapsed());

    perform_transactions(
        &mut tester,
        rich_account.clone(),
        token,
        fee_token,
        deposit_amount.clone(),
        contracts.zk_sync,
        config.clone(),
        BlockProcessing::CommitAndExecute,
    )
    .await;

    tester.assert_balances_correctness().await;
}
