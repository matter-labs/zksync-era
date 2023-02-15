use num::BigUint;
use std::convert::TryFrom;
use tempfile::TempDir;
use zksync_storage::RocksDB;
use zksync_types::web3::{transports::Http, types::FilterBuilder};

use crate::external_commands::{get_test_accounts, Contracts};
use crate::tester::Tester;
use crate::types::{AccountHandler, BlockProcessing};
use crate::utils::load_test_bytecode_and_calldata;
use zksync_config::ZkSyncConfig;
use zksync_contracts::zksync_contract;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_storage::db::Database;
use zksync_types::{l1::L1Tx, tokens::ETHEREUM_ADDRESS, Address, L1BatchNumber, H256};

pub fn get_test_config() -> ZkSyncConfig {
    let mut config = ZkSyncConfig::from_env();
    config.chain.operations_manager.delay_interval = 0;
    config.chain.state_keeper.block_commit_deadline_ms = u64::MAX;
    config.eth_sender.sender.aggregated_block_commit_deadline = u64::MAX;
    config.eth_sender.sender.aggregated_block_prove_deadline = u64::MAX;
    config.eth_sender.sender.aggregated_block_execute_deadline = u64::MAX;
    config.eth_sender.sender.max_aggregated_blocks_to_commit = 1;
    config.eth_sender.sender.max_aggregated_blocks_to_execute = 1;
    config.eth_sender.sender.aggregated_proof_sizes = vec![1];

    config
}

pub struct TestDatabaseManager {
    temp_dir: Option<TempDir>,
    state_keeper_temp_dir: Option<TempDir>,
}

impl TestDatabaseManager {
    pub async fn new() -> Self {
        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDb");
        let state_keeper_temp_dir =
            TempDir::new().expect("failed get temporary directory for RocksDb");
        Self {
            temp_dir: Some(temp_dir),
            state_keeper_temp_dir: Some(state_keeper_temp_dir),
        }
    }

    pub fn get_db_path(&self) -> String {
        self.temp_dir
            .as_ref()
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string()
    }

    pub fn get_state_keeper_db(&self) -> RocksDB {
        RocksDB::new(
            Database::StateKeeper,
            self.state_keeper_temp_dir.as_ref().unwrap(),
            false,
        )
    }

    pub async fn connect_to_postgres(&self) -> StorageProcessor<'static> {
        // This method is currently redundant, but it was created so that if some tweaks would be required,
        // it'll be easier to introduce them through `TestDatabaseManager`.
        StorageProcessor::establish_connection(true).await
    }

    pub fn create_pool(&self) -> ConnectionPool {
        // This method is currently redundant, but it was created so that if some tweaks would be required,
        // it'll be easier to introduce them through `TestDatabaseManager`.
        ConnectionPool::new(Some(1), true)
    }
}

impl Drop for TestDatabaseManager {
    fn drop(&mut self) {
        let temp_dir = self.temp_dir.take();
        if let Some(temp_dir) = temp_dir {
            temp_dir
                .close()
                .expect("failed close temporary file for RocksDb");
        }
    }
}

pub fn get_root_hash(storage: &mut StorageProcessor<'static>) -> H256 {
    let metadata = storage
        .blocks_dal()
        .get_block_metadata(L1BatchNumber(0))
        .unwrap();
    metadata.metadata.root_hash
}

/// Return Operator account and rich zksync account with balance in L1.
pub fn create_test_accounts(
    config: &ZkSyncConfig,
    contracts: &Contracts,
) -> (AccountHandler, AccountHandler) {
    let transport = Http::new(&config.eth_client.web3_url).expect("http transport start");
    let (test_accounts_info, operator_account_info) = get_test_accounts();

    let operator_account = AccountHandler::new(
        operator_account_info.private_key,
        transport.clone(),
        config,
        contracts.zk_sync,
    );
    let rich_account = test_accounts_info
        .last()
        .map(|test_eth_account| {
            AccountHandler::new(
                test_eth_account.private_key,
                transport.clone(),
                config,
                contracts.zk_sync,
            )
        })
        .expect("can't use testkit without rich test account");

    (operator_account, rich_account)
}

/// Eth is added to the contract as a first class citizen token during deployment,
/// so the priority operation of adding a token must be processed separately.
async fn add_eth_token(tester: &mut Tester, account: &AccountHandler, contract_address: Address) {
    let new_priority_req_event_topic = zksync_contract()
        .event("NewPriorityRequest")
        .expect("zkSync contract abi error")
        .signature();
    let filter = FilterBuilder::default()
        .address(vec![contract_address])
        .from_block(0.into())
        .topics(Some(vec![new_priority_req_event_topic]), None, None, None)
        .build();
    let logs = account
        .eth_provider
        .main_contract_eth_client
        .logs(filter, "utils")
        .await
        .unwrap();

    let tx = logs
        .iter()
        .find_map(|op| L1Tx::try_from(op.clone()).ok())
        .expect("failed get L1 tx from logs");

    tester.add_tx_to_mempool(tx.into()).await;
}

pub async fn create_first_block(tester: &mut Tester, mut config: ZkSyncConfig) {
    println!("Create block with add tokens");
    config.chain.state_keeper.transaction_slots = 4;
    tester.change_config(config).await;

    let operator_account = tester.operator_account.clone();

    tester
        .add_custom_token(
            &operator_account,
            ETHEREUM_ADDRESS,
            "ETH".to_string(),
            "ETH".to_string(),
            18,
        )
        .await;
    tester
        .deposit(
            &operator_account,
            &operator_account,
            ETHEREUM_ADDRESS,
            BigUint::from(10u32).pow(20),
        )
        .await;

    tester.load_operations(1).await;

    // Commit blocks
    let (block_range, exec_result) = tester.commit_blocks().await.unwrap();
    let tx_receipt = exec_result.expect_success();
    println!(
        "Commit blocks, block range: {:?} tx hash: {:?}",
        block_range, tx_receipt.transaction_hash
    );

    // Verify blocks
    let (block_range, exec_result) = tester.verify_blocks().await.unwrap();
    let tx_receipt = exec_result.expect_success();
    println!(
        "Verify blocks, block range: {:?} tx hash: {:?}",
        block_range, tx_receipt.transaction_hash
    );

    // Execute blocks
    let (block_range, exec_result) = tester.execute_blocks().await.unwrap();
    let tx_receipt = exec_result.expect_success();
    println!(
        "Execute blocks, block range: {:?} tx hash: {:?}",
        block_range, tx_receipt.transaction_hash
    );
}

#[allow(clippy::too_many_arguments)]
pub async fn perform_transactions(
    tester: &mut Tester,
    rich_account: AccountHandler,
    token: Address,
    fee_token: Address,
    deposit_amount: BigUint,
    zksync_contract: Address,
    mut config: ZkSyncConfig,
    block_proccesing: BlockProcessing,
) {
    config.chain.state_keeper.transaction_slots = 8;
    tester.change_config(config.clone()).await;

    let fee_amount = std::cmp::min(
        &deposit_amount / BigUint::from(100u32),
        BigUint::from(u32::MAX),
    );

    let alice = AccountHandler::rand(&config, zksync_contract);
    let bob = AccountHandler::rand(&config, zksync_contract);

    tester
        .deposit(&rich_account, &alice, fee_token, deposit_amount.clone())
        .await;
    println!(
        "Deposit to other account test success, token address: {}",
        token
    );
    tester
        .deposit(&rich_account, &alice, token, deposit_amount.clone())
        .await;
    println!(
        "Deposit to other account test success, token address: {}",
        fee_token
    );

    tester
        .transfer(
            &alice,
            &alice,
            token,
            fee_amount.clone(),
            fee_amount.clone(),
        )
        .await;
    println!("Transfer to self test success");

    tester
        .transfer(
            &alice,
            &bob,
            token,
            &deposit_amount / BigUint::from(2u32),
            fee_amount.clone(),
        )
        .await;
    tester
        .transfer(
            &alice,
            &bob,
            ETHEREUM_ADDRESS,
            fee_amount.clone(),
            fee_amount.clone(),
        )
        .await;
    println!("Transfer to other test success");

    tester
        .withdraw(
            &bob,
            &bob,
            ETHEREUM_ADDRESS,
            fee_amount.clone(),
            fee_amount.clone(),
        )
        .await;
    println!("Withdraw to self test success");

    let (bytecode, constructor_calldata, calldata) = load_test_bytecode_and_calldata();

    let contract_address = tester
        .deploy_contract(
            &alice,
            bytecode,
            constructor_calldata,
            fee_amount.clone(),
            0.into(),
        )
        .await;
    println!("Deploy contract test success");

    tester
        .execute_contract(&alice, contract_address, calldata, fee_amount.clone())
        .await;
    println!("Execute contract test success");

    tester.load_operations(1).await;

    // Commit blocks
    let (block_range, exec_result) = tester.commit_blocks().await.unwrap();
    let tx_receipt = exec_result.expect_success();
    println!(
        "Commit blocks, block range: {:?}, tx hash: {:#x}",
        block_range, tx_receipt.transaction_hash
    );

    match block_proccesing {
        BlockProcessing::CommitAndExecute => {
            // Verify blocks
            let (block_range, exec_result) = tester.verify_blocks().await.unwrap();
            let tx_receipt = exec_result.expect_success();
            println!(
                "Verify blocks, block range: {:?}, tx hash: {:#x}",
                block_range, tx_receipt.transaction_hash
            );

            // Execute blocks
            let (block_range, exec_result) = tester.execute_blocks().await.unwrap();
            let tx_receipt = exec_result.expect_success();
            println!(
                "Execute blocks, block range: {:?}, tx hash: {:#x}",
                block_range, tx_receipt.transaction_hash
            );
        }
        BlockProcessing::CommitAndRevert => {
            // Revert blocks
            let (block_range, exec_result) = tester.revert_blocks().await.unwrap();
            let tx_receipt = exec_result.expect_success();
            println!(
                "Revert blocks,block range: {:?}, tx hash: {:#x}",
                block_range, tx_receipt.transaction_hash
            );
        }
    }
}
