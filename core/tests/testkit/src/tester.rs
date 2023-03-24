use num::{bigint::Sign, BigUint};
use std::time::Instant;
use zksync_config::ZkSyncConfig;
use zksync_types::{
    web3::types::TransactionReceipt, Address, ExecuteTransactionCommon, L1BatchNumber, Transaction,
    H256, U256,
};
use zksync_utils::{biguint_to_u256, u256_to_biguint};

use crate::commands::utils::TestDatabaseManager;
use crate::eth_provider::EthExecResult;
use crate::server_handler::ServerHandler;
use crate::types::{
    AccountBalances, AccountHandler, BalanceUpdate, LayerType, OperationsQueue, ETHEREUM_ADDRESS,
    VERY_BIG_BLOCK_NUMBER,
};
use crate::utils::{get_executed_tx_fee, is_token_eth, l1_tx_from_logs};
use zksync_contracts::erc20_contract;
use zksync_dal::StorageProcessor;
use zksync_storage::RocksDB;
use zksync_types::{
    aggregated_operations::BlocksCommitOperation,
    api::web3::contract::tokens::Tokenize,
    fee::Fee,
    l1::{L1Tx, OpProcessingType, PriorityQueueType},
    l2::L2Tx,
    utils::deployed_address_create,
};

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct State {
    pub last_committed_block: L1BatchNumber,
    pub last_executed_block: L1BatchNumber,
    pub last_proved_block: L1BatchNumber,
}

pub struct Tester {
    pub db_manager: TestDatabaseManager,
    pub storage: StorageProcessor<'static>,
    pub operator_account: AccountHandler,
    operations_queue: OperationsQueue,
    account_balances: AccountBalances,
    server_handler: ServerHandler,
    state: State,
    config: ZkSyncConfig,
}

impl Tester {
    pub async fn new(
        db_manager: TestDatabaseManager,
        operator_account: AccountHandler,
        config: ZkSyncConfig,
    ) -> Self {
        let state = State::default();
        let server_handler = ServerHandler::spawn_server(
            db_manager.get_db_path(),
            db_manager.get_state_keeper_db(),
            config.clone(),
            db_manager.create_pool(),
            db_manager.create_pool(),
            db_manager.create_pool(),
        );

        let storage = db_manager.connect_to_postgres().await;
        Self {
            db_manager,
            storage,
            operator_account: operator_account.clone(),
            account_balances: AccountBalances::new(operator_account),
            operations_queue: Default::default(),
            server_handler,
            state,
            config,
        }
    }

    pub async fn change_config(&mut self, config: ZkSyncConfig) {
        // Server handler should be dropped firstly before spawning new server.
        self.server_handler = ServerHandler::empty();
        RocksDB::await_rocksdb_termination();
        self.server_handler = ServerHandler::spawn_server(
            self.db_manager.get_db_path(),
            self.db_manager.get_state_keeper_db(),
            config,
            self.db_manager.create_pool(),
            self.db_manager.create_pool(),
            self.db_manager.create_pool(),
        );
    }

    pub async fn add_tx_to_mempool(&mut self, tx: Transaction) {
        let Transaction {
            common_data,
            execute,
            received_timestamp_ms,
        } = tx;
        match common_data {
            ExecuteTransactionCommon::L2(mut common_data) => {
                common_data.set_input(H256::random().0.to_vec(), H256::random());
                self.storage.transactions_dal().insert_transaction_l2(
                    L2Tx {
                        execute,
                        common_data,
                        received_timestamp_ms,
                    },
                    Default::default(),
                );
            }
            ExecuteTransactionCommon::L1(common_data) => {
                self.storage.transactions_dal().insert_transaction_l1(
                    L1Tx {
                        execute,
                        common_data,
                        received_timestamp_ms,
                    },
                    0.into(),
                );
            }
        }
    }

    async fn approve_erc20_if_need(
        &mut self,
        from: &AccountHandler,
        token: Address,
        amount: BigUint,
    ) {
        let allowance = from
            .eth_provider
            .allowance(token)
            .await
            .map(u256_to_biguint)
            .unwrap();

        if allowance < amount {
            self.account_balances
                .setup_balances(&mut self.storage, &[(from.clone(), ETHEREUM_ADDRESS)])
                .await;

            let execution_result = from
                .eth_provider
                .approve_erc20(token, u256_to_biguint(U256::max_value()))
                .await
                .expect("erc20 approve should not fail");
            let tx_receipt = execution_result.expect_success();
            let fee = get_executed_tx_fee(&from.eth_provider.main_contract_eth_client, &tx_receipt)
                .await
                .unwrap();

            self.account_balances.update_balance(
                from.address(),
                ETHEREUM_ADDRESS,
                BalanceUpdate::new(LayerType::Ethereum, Sign::Minus, fee),
            );
        }
    }

    pub async fn deposit(
        &mut self,
        from: &AccountHandler,
        to: &AccountHandler,
        token: Address,
        amount: BigUint,
    ) -> TransactionReceipt {
        self.account_balances
            .setup_balances(
                &mut self.storage,
                &[
                    (from.clone(), token),
                    (to.clone(), token),
                    (from.clone(), ETHEREUM_ADDRESS),
                ],
            )
            .await;

        let execution_result = if is_token_eth(token) {
            from.eth_provider
                .deposit_eth(
                    amount.clone(),
                    &to.address(),
                    PriorityQueueType::Deque,
                    OpProcessingType::Common,
                    Default::default(),
                )
                .await
                .expect("eth deposit should not fail")
        } else {
            self.approve_erc20_if_need(from, token, amount.clone())
                .await;
            from.eth_provider
                .deposit_erc20(
                    token,
                    amount.clone(),
                    &to.address(),
                    PriorityQueueType::Deque,
                    OpProcessingType::Common,
                    Default::default(),
                )
                .await
                .expect("erc20 deposit should not fail")
        };
        let tx_receipt = execution_result.expect_success();
        let fee = get_executed_tx_fee(&from.eth_provider.main_contract_eth_client, &tx_receipt)
            .await
            .unwrap();
        let deposit = l1_tx_from_logs(&tx_receipt);
        self.add_tx_to_mempool(deposit.into()).await;

        self.account_balances.update_balance(
            from.address(),
            ETHEREUM_ADDRESS,
            BalanceUpdate::new(LayerType::Ethereum, Sign::Minus, fee),
        );
        self.account_balances.update_balance(
            from.address(),
            token,
            BalanceUpdate::new(LayerType::Ethereum, Sign::Minus, amount.clone()),
        );
        self.account_balances.update_balance(
            to.address(),
            token,
            BalanceUpdate::new(LayerType::Zksync, Sign::Plus, amount),
        );

        tx_receipt
    }

    pub async fn add_custom_token(
        &mut self,
        from: &AccountHandler,
        token_address: Address,
        name: String,
        symbol: String,
        decimals: u8,
    ) -> TransactionReceipt {
        let execution_result = from
            .eth_provider
            .add_custom_token(
                token_address,
                name,
                symbol,
                decimals,
                PriorityQueueType::Deque,
                OpProcessingType::Common,
                Default::default(),
            )
            .await
            .expect("add token should not fail");
        let tx_receipt = execution_result.expect_success();
        let add_token = l1_tx_from_logs(&tx_receipt);
        self.add_tx_to_mempool(add_token.into()).await;

        tx_receipt
    }

    pub async fn add_token(
        &mut self,
        from: &AccountHandler,
        token_address: Address,
    ) -> TransactionReceipt {
        let execution_result = from
            .eth_provider
            .add_token(
                token_address,
                PriorityQueueType::Deque,
                OpProcessingType::Common,
                Default::default(),
            )
            .await
            .expect("add token should not fail");
        let tx_receipt = execution_result.expect_success();
        let add_token = l1_tx_from_logs(&tx_receipt);
        self.add_tx_to_mempool(add_token.into()).await;

        tx_receipt
    }

    pub async fn transfer(
        &mut self,
        from: &AccountHandler,
        to: &AccountHandler,
        token: Address,
        amount: BigUint,
        fee: BigUint,
    ) {
        self.account_balances
            .setup_balances(
                &mut self.storage,
                &[
                    (from.clone(), token),
                    (to.clone(), token),
                    (from.clone(), ETHEREUM_ADDRESS),
                    (to.clone(), ETHEREUM_ADDRESS),
                ],
            )
            .await;

        let data =
            create_transfer_calldata(to.sync_account.address, biguint_to_u256(amount.clone()));

        let fee = Fee {
            gas_limit: biguint_to_u256(fee.clone()),
            max_fee_per_gas: 1u32.into(),
            max_priority_fee_per_gas: 1u32.into(),
            gas_per_pubdata_limit: Default::default(),
        };

        let signed_tx = from
            .sync_account
            .sign_execute(token, data, fee.clone(), None, true);

        self.add_tx_to_mempool(signed_tx.into()).await;

        self.account_balances.update_balance(
            from.address(),
            token,
            BalanceUpdate::new(LayerType::Zksync, Sign::Minus, amount.clone()),
        );
        self.account_balances.update_balance(
            from.address(),
            ETHEREUM_ADDRESS,
            BalanceUpdate::new(
                LayerType::Zksync,
                Sign::Minus,
                u256_to_biguint(fee.max_total_fee()),
            ),
        );
        self.account_balances.update_balance(
            to.address(),
            token,
            BalanceUpdate::new(LayerType::Zksync, Sign::Plus, amount),
        );
    }

    pub async fn withdraw(
        &mut self,
        from: &AccountHandler,
        to: &AccountHandler,
        token: Address,
        amount: BigUint,
        fee: BigUint,
    ) {
        self.account_balances
            .setup_balances(
                &mut self.storage,
                &[
                    (from.clone(), token),
                    (to.clone(), token),
                    (from.clone(), ETHEREUM_ADDRESS),
                ],
            )
            .await;
        let fee = Fee {
            gas_limit: biguint_to_u256(fee.clone()),
            max_fee_per_gas: 1u32.into(),
            max_priority_fee_per_gas: 1u32.into(),
            gas_per_pubdata_limit: Default::default(),
        };
        let signed_tx = from.sync_account.sign_withdraw(
            token,
            biguint_to_u256(amount.clone()),
            fee.clone(),
            to.address(),
            None,
            true,
        );
        self.add_tx_to_mempool(signed_tx.into()).await;

        self.account_balances.update_balance(
            from.address(),
            token,
            BalanceUpdate::new(LayerType::Zksync, Sign::Minus, amount.clone()),
        );
        self.account_balances.update_balance(
            from.address(),
            ETHEREUM_ADDRESS,
            BalanceUpdate::new(
                LayerType::Zksync,
                Sign::Minus,
                u256_to_biguint(fee.max_total_fee()),
            ),
        );
        self.account_balances.update_balance(
            to.address(),
            token,
            BalanceUpdate::new(LayerType::Ethereum, Sign::Plus, amount),
        );
    }

    pub async fn deploy_contract(
        &mut self,
        from: &AccountHandler,
        bytecode: Vec<u8>,
        calldata: Vec<u8>,
        fee: BigUint,
        amount_of_contracts_deployed_by_address: U256,
    ) -> Address {
        self.account_balances
            .setup_balances(&mut self.storage, &[(from.clone(), ETHEREUM_ADDRESS)])
            .await;

        let fee = Fee {
            gas_limit: biguint_to_u256(fee.clone()),
            max_fee_per_gas: 1u32.into(),
            max_priority_fee_per_gas: 1u32.into(),
            gas_per_pubdata_limit: Default::default(),
        };
        let signed_tx =
            from.sync_account
                .sign_deploy_contract(bytecode, calldata, fee.clone(), None, true);

        let contract_address = deployed_address_create(
            signed_tx.initiator_account(),
            amount_of_contracts_deployed_by_address,
        );
        self.add_tx_to_mempool(signed_tx.into()).await;

        self.account_balances.update_balance(
            from.address(),
            ETHEREUM_ADDRESS,
            BalanceUpdate::new(
                LayerType::Zksync,
                Sign::Minus,
                u256_to_biguint(fee.max_total_fee()),
            ),
        );

        contract_address
    }

    pub async fn execute_contract(
        &mut self,
        from: &AccountHandler,
        contract_address: Address,
        calldata: Vec<u8>,
        fee: BigUint,
    ) {
        self.account_balances
            .setup_balances(&mut self.storage, &[(from.clone(), ETHEREUM_ADDRESS)])
            .await;

        let fee = Fee {
            gas_limit: biguint_to_u256(fee.clone()),
            max_fee_per_gas: 1u32.into(),
            max_priority_fee_per_gas: 1u32.into(),
            gas_per_pubdata_limit: Default::default(),
        };
        let signed_tx =
            from.sync_account
                .sign_execute(contract_address, calldata, fee.clone(), None, true);
        self.add_tx_to_mempool(signed_tx.into()).await;
        self.account_balances.update_balance(
            from.address(),
            ETHEREUM_ADDRESS,
            BalanceUpdate::new(
                LayerType::Zksync,
                Sign::Minus,
                u256_to_biguint(fee.max_total_fee()),
            ),
        );
    }

    pub async fn load_operations(&mut self, limit: usize) {
        let start = Instant::now();
        let blocks = loop {
            if start.elapsed().as_secs() > 20 {
                panic!("Expect load new operation");
            }
            let all_blocks = self.storage.blocks_dal().get_ready_for_commit_blocks(
                VERY_BIG_BLOCK_NUMBER.0 as usize,
                self.config.chain.state_keeper.bootloader_hash,
                self.config.chain.state_keeper.default_aa_hash,
            );
            let blocks: Vec<_> = all_blocks
                .into_iter()
                .filter(|block| block.header.number > self.state.last_committed_block)
                .take(limit)
                .collect();
            if blocks.len() == limit {
                break blocks;
            }
        };
        let last_committed_block = self
            .storage
            .blocks_dal()
            .get_block_metadata(self.state.last_committed_block)
            .unwrap();
        let commit_op = BlocksCommitOperation {
            blocks,
            last_committed_block,
        };
        self.operations_queue.add_commit_op(commit_op);
    }

    pub async fn commit_blocks(
        &mut self,
    ) -> Option<((L1BatchNumber, L1BatchNumber), EthExecResult)> {
        let block_commit_op = self.operations_queue.get_commit_op();
        if let Some(block_commit_op) = block_commit_op {
            let block_range = block_commit_op.block_range();
            let exec_result = self
                .operator_account
                .eth_provider
                .commit_blocks(&block_commit_op)
                .await
                .expect("commit block tx");
            self.state.last_committed_block = block_range.1;

            Some((block_range, exec_result))
        } else {
            None
        }
    }

    pub async fn verify_blocks(
        &mut self,
    ) -> Option<((L1BatchNumber, L1BatchNumber), EthExecResult)> {
        let block_verify_op = self.operations_queue.get_verify_op();
        if let Some(block_verify_op) = block_verify_op {
            let block_range = block_verify_op.block_range();
            let exec_result = self
                .operator_account
                .eth_provider
                .verify_blocks(&block_verify_op)
                .await
                .expect("verify block tx");
            self.state.last_proved_block = block_range.1;

            Some((block_range, exec_result))
        } else {
            None
        }
    }

    pub async fn execute_blocks(
        &mut self,
    ) -> Option<((L1BatchNumber, L1BatchNumber), EthExecResult)> {
        let block_execute_op = self.operations_queue.get_execute_op();
        if let Some(block_execute_op) = block_execute_op {
            let block_range = block_execute_op.block_range();
            let exec_result = self
                .operator_account
                .eth_provider
                .execute_blocks(&block_execute_op)
                .await
                .expect("execute block tx");
            self.state.last_executed_block = block_range.1;

            Some((block_range, exec_result))
        } else {
            None
        }
    }

    pub async fn revert_blocks(&mut self) -> Option<(L1BatchNumber, EthExecResult)> {
        let blocks = self.operations_queue.revert_blocks();
        if let Some(blocks) = blocks {
            let last_committed_block = blocks[0].header.number - 1;

            let exec_result = self
                .operator_account
                .eth_provider
                .revert_blocks(*last_committed_block)
                .await
                .expect("execute block tx");

            Some((last_committed_block, exec_result))
        } else {
            None
        }
    }

    pub async fn assert_balances_correctness(&mut self) {
        let mut checks_failed = false;
        for ((eth_account, token_address), expected_balance) in
            &self.account_balances.eth_accounts_balances
        {
            let real_balance = self
                .account_balances
                .get_real_balance(
                    &mut self.storage,
                    *eth_account,
                    *token_address,
                    LayerType::Ethereum,
                )
                .await;

            if expected_balance != &real_balance {
                println!(
                    "eth acc: {:x}, token address: {:x}",
                    *eth_account, token_address
                );
                println!("expected: {}", expected_balance);
                println!("real:     {}", real_balance);
                checks_failed = true;
            }
        }

        for ((zksync_account, token_address), expected_balance) in
            &self.account_balances.sync_accounts_balances
        {
            let real_balance = self
                .account_balances
                .get_real_balance(
                    &mut self.storage,
                    *zksync_account,
                    *token_address,
                    LayerType::Zksync,
                )
                .await;

            if &real_balance != expected_balance {
                println!(
                    "zkSync acc: {:x}, token address: {:x}",
                    zksync_account, token_address
                );
                println!("expected:   {}", expected_balance);
                println!("real:       {}", real_balance);
                checks_failed = true;
            }
        }

        assert!(!checks_failed, "failed check balances correctness");
    }
}
pub fn create_transfer_calldata(to: Address, amount: U256) -> Vec<u8> {
    let contract = erc20_contract();
    let contract_function = contract
        .function("transfer")
        .expect("failed to get function parameters");
    let params = (to, amount);
    contract_function
        .encode_input(&params.into_tokens())
        .expect("failed to encode parameters")
}
