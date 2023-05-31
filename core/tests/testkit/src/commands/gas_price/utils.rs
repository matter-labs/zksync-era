use num::{bigint::RandBigInt, BigUint};

use zksync_config::ZkSyncConfig;
use zksync_types::{Address, U256};

use crate::commands::gas_price::types::{BaseCost, BlockExecutionResult, CostsSample};
use crate::tester::Tester;
use crate::types::{AccountHandler, ETHEREUM_ADDRESS};

async fn process_blocks(
    tester: &mut Tester,
    expected_aggregated_blocks: u32,
) -> BlockExecutionResult {
    // Expect that there are enough transactions for the block to close.
    tester
        .load_operations(expected_aggregated_blocks as usize)
        .await;

    let (commit_block_range, commit_result) = tester
        .commit_blocks()
        .await
        .expect("Commit blocks op failed");
    let (verify_block_range, verify_result) = tester
        .verify_blocks()
        .await
        .expect("Verify blocks op failed");
    let (execute_block_range, execute_result) = tester
        .execute_blocks()
        .await
        .expect("Execute blocks op failed");

    assert_eq!(
        commit_block_range, verify_block_range,
        "Not the same blocks are committed and verified, committed: {:?}, verified: {:?}",
        commit_block_range, verify_block_range
    );
    assert_eq!(
        verify_block_range, execute_block_range,
        "Not the same blocks are verified and executed, verified: {:?}, executed: {:?}",
        commit_block_range, execute_block_range
    );
    assert_eq!(
        *execute_block_range.1 - *execute_block_range.0 + 1,
        expected_aggregated_blocks,
        "The number of aggregated blocks is not as expected: real: {}, expected: {}",
        *execute_block_range.1 - *execute_block_range.0 + 1,
        expected_aggregated_blocks
    );

    BlockExecutionResult::new(
        commit_result.expect_success(),
        verify_result.expect_success(),
        execute_result.expect_success(),
    )
}

pub async fn commit_cost_of_n_empty_blocks(
    tester: &mut Tester,
    n: u32,
    mut config: ZkSyncConfig,
    zksync_contract: Address,
) -> BaseCost {
    let operator_account = tester.operator_account.clone();

    config.chain.state_keeper.transaction_slots = 1;
    tester.change_config(config.clone()).await;

    for _ in 0..n {
        let random_account = AccountHandler::rand(&config, zksync_contract);
        // In order for the block to close, we should initialize the transaction,
        // So far, we will use one transfer operation, but in the future it will need to be replaced with noop.
        tester
            .transfer(
                &operator_account,
                &random_account,
                ETHEREUM_ADDRESS,
                BigUint::from(10u32).pow(17),
                BigUint::from(u32::MAX),
            )
            .await;
    }
    tester.operator_account = operator_account;
    let block_execution_res = process_blocks(tester, n).await;

    BaseCost::from_block_execution_result(block_execution_res)
}

pub async fn commit_cost_of_deposits(
    tester: &mut Tester,
    n_operations: usize,
    token: Address,
    mut config: ZkSyncConfig,
    zksync_contract: Address,
    rng: &mut impl RandBigInt,
) -> CostsSample {
    config.chain.state_keeper.transaction_slots = n_operations;
    tester.change_config(config.clone()).await;

    let mut user_gas_cost = U256::zero();
    let operator_account = tester.operator_account.clone();
    for _ in 0..n_operations {
        let random_account = AccountHandler::rand(&config, zksync_contract);
        let deposit_tx_receipt = tester
            .deposit(
                &operator_account,
                &random_account,
                token,
                rng.gen_biguint_range(&BigUint::from(10u32).pow(17), &BigUint::from(10u32).pow(18)),
            )
            .await;
        user_gas_cost += deposit_tx_receipt.gas_used.expect("deposit gas used");
    }

    let deposits_execute_result = process_blocks(tester, 1).await;

    CostsSample::new(n_operations, user_gas_cost, deposits_execute_result)
}

pub async fn commit_cost_of_transfers_to_new(
    tester: &mut Tester,
    n_operations: usize,
    token: Address,
    mut config: ZkSyncConfig,
    zksync_contract: Address,
    rng: &mut impl RandBigInt,
) -> CostsSample {
    config.chain.state_keeper.transaction_slots = n_operations;
    tester.change_config(config.clone()).await;

    let operator_account = tester.operator_account.clone();
    for _ in 0..n_operations {
        let random_account = AccountHandler::rand(&config, zksync_contract);
        tester
            .transfer(
                &operator_account,
                &random_account,
                token,
                rng.gen_biguint_range(&BigUint::from(10u32).pow(17), &BigUint::from(10u32).pow(18)),
                rng.gen_biguint_range(&BigUint::from(u32::MAX / 2), &BigUint::from(u32::MAX)),
            )
            .await;
    }
    tester.operator_account = operator_account;

    let transfers_execute_result = process_blocks(tester, 1).await;
    CostsSample::new(n_operations, U256::zero(), transfers_execute_result)
}

pub async fn commit_cost_of_transfers(
    tester: &mut Tester,
    n_operations: usize,
    token: Address,
    mut config: ZkSyncConfig,
    zksync_contract: Address,
    rng: &mut impl RandBigInt,
) -> CostsSample {
    config.chain.state_keeper.transaction_slots = n_operations;
    tester.change_config(config.clone()).await;

    let operator_account = tester.operator_account.clone();

    let mut accounts = Vec::with_capacity(n_operations);
    for _ in 0..n_operations {
        let random_account = AccountHandler::rand(&config, zksync_contract);
        tester
            .deposit(
                &operator_account,
                &random_account,
                token,
                rng.gen_biguint_range(&BigUint::from(10u32).pow(17), &BigUint::from(10u32).pow(18)),
            )
            .await;

        accounts.push(random_account);
    }
    process_blocks(tester, 1).await;

    for test_account in accounts {
        tester
            .transfer(
                &operator_account,
                &test_account,
                token,
                rng.gen_biguint_range(&BigUint::from(10u32).pow(17), &BigUint::from(10u32).pow(18)),
                rng.gen_biguint_range(&BigUint::from(u32::MAX / 2), &BigUint::from(u32::MAX)),
            )
            .await;
    }
    tester.operator_account = operator_account;

    let transfers_execute_result = process_blocks(tester, 1).await;

    CostsSample::new(n_operations, U256::zero(), transfers_execute_result)
}

pub async fn commit_cost_of_withdrawals(
    tester: &mut Tester,
    n_operations: usize,
    token: Address,
    mut config: ZkSyncConfig,
    zksync_contract: Address,
    rng: &mut impl RandBigInt,
) -> CostsSample {
    config.chain.state_keeper.transaction_slots = n_operations;
    // It is needed to avoid closing block because of the gas limit.
    // Constant for executing ERC20 withdrawals is quite high.
    config.chain.state_keeper.max_single_tx_gas = 5000000;
    tester.change_config(config.clone()).await;

    let operator_account = tester.operator_account.clone();

    let mut accounts = Vec::with_capacity(n_operations);
    for _ in 0..n_operations {
        let random_account = AccountHandler::rand(&config, zksync_contract);
        tester
            .deposit(
                &operator_account,
                &random_account,
                token,
                rng.gen_biguint_range(&BigUint::from(10u32).pow(17), &BigUint::from(10u32).pow(18)),
            )
            .await;

        accounts.push(random_account);
    }
    process_blocks(tester, 1).await;

    for test_account in accounts {
        tester
            .withdraw(
                &operator_account,
                &test_account,
                token,
                rng.gen_biguint_range(&BigUint::from(10u32).pow(16), &BigUint::from(10u32).pow(17)),
                rng.gen_biguint_range(&BigUint::from(u32::MAX / 2), &BigUint::from(u32::MAX)),
            )
            .await;
    }
    tester.operator_account = operator_account;

    let withdrawals_execute_result = process_blocks(tester, 1).await;

    CostsSample::new(n_operations, U256::zero(), withdrawals_execute_result)
}

pub async fn commit_cost_of_deploys(
    tester: &mut Tester,
    n_operations: usize,
    bytecode: Vec<u8>,
    constructor_calldata: Vec<u8>,
    mut config: ZkSyncConfig,
    rng: &mut impl RandBigInt,
) -> CostsSample {
    config.chain.state_keeper.transaction_slots = n_operations;
    tester.change_config(config.clone()).await;

    let operator_account = tester.operator_account.clone();
    for n_accounts in 0..n_operations {
        tester
            .deploy_contract(
                &operator_account,
                bytecode.clone(),
                constructor_calldata.clone(),
                rng.gen_biguint_range(&BigUint::from(u32::MAX / 2), &BigUint::from(u32::MAX)),
                n_accounts.into(), // TODO: May be incorrect, needs to be checked.
            )
            .await;
    }
    tester.operator_account = operator_account;

    let deploys_execute_result = process_blocks(tester, 1).await;
    CostsSample::new(n_operations, U256::zero(), deploys_execute_result)
}

#[allow(clippy::too_many_arguments)]
pub async fn commit_cost_of_executes(
    tester: &mut Tester,
    n_operations: usize,
    bytecode: Vec<u8>,
    constructor_calldata: Vec<u8>,
    calldata: Vec<u8>,
    mut config: ZkSyncConfig,
    rng: &mut impl RandBigInt,
) -> CostsSample {
    config.chain.state_keeper.transaction_slots = 1;
    tester.change_config(config.clone()).await;
    let operator_account = tester.operator_account.clone();

    let address = tester
        .deploy_contract(
            &operator_account,
            bytecode.clone(),
            constructor_calldata.clone(),
            rng.gen_biguint_range(&BigUint::from(u32::MAX / 2), &BigUint::from(u32::MAX)),
            0.into(), // TODO: May be incorrect, needs to be checked.
        )
        .await;
    process_blocks(tester, 1).await;

    config.chain.state_keeper.transaction_slots = n_operations;
    tester.change_config(config.clone()).await;

    for _ in 0..n_operations {
        tester
            .execute_contract(
                &operator_account,
                address,
                calldata.clone(),
                rng.gen_biguint_range(&BigUint::from(u32::MAX / 2), &BigUint::from(u32::MAX)),
            )
            .await;
    }
    tester.operator_account = operator_account;

    let executes_execute_result = process_blocks(tester, 1).await;
    CostsSample::new(n_operations, U256::zero(), executes_execute_result)
}

pub async fn commit_cost_of_add_tokens(
    tester: &mut Tester,
    addresses: Vec<Address>,
    mut config: ZkSyncConfig,
) -> CostsSample {
    let n_operations = addresses.len();
    config.chain.state_keeper.transaction_slots = n_operations;
    tester.change_config(config.clone()).await;

    let operator_account = tester.operator_account.clone();
    for address in addresses {
        tester.add_token(&operator_account, address).await;
    }
    tester.operator_account = operator_account;

    let add_tokens_execute_result = process_blocks(tester, 1).await;
    CostsSample::new(n_operations, U256::zero(), add_tokens_execute_result)
}
