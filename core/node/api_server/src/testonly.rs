//! Test utils shared among multiple modules.

use std::collections::HashMap;

use assert_matches::assert_matches;
use zk_evm_1_5_0::zkevm_opcode_defs::decoding::{EncodingModeProduction, VmEncodingMode};
use zksync_contracts::{load_contract, read_bytecode};
use zksync_dal::{
    transactions_dal::L2TxSubmissionResult, Connection, ConnectionPool, Core, CoreDal,
};
use zksync_multivm::{
    interface::{
        tracer::ValidationTraces, ExecutionResult, TransactionExecutionMetrics,
        TransactionExecutionResult, TxExecutionStatus, VmExecutionMetrics,
    },
    utils::{derive_base_fee_and_gas_per_pubdata, StorageWritesDeduplicator},
};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{create_l2_block, default_l1_batch_env, default_system_env};
use zksync_state::PostgresStorage;
use zksync_system_constants::{
    CONTRACT_DEPLOYER_ADDRESS, L2_BASE_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS,
    REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, SYSTEM_CONTEXT_ADDRESS,
    SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
};
use zksync_test_contracts::{
    Account, LoadnextContractExecutionParams, TestContract, TestEvmContract,
};
use zksync_types::{
    address_to_u256,
    api::state_override::{BytecodeOverride, OverrideAccount, OverrideState, StateOverride},
    block::{pack_block_info, L2BlockHeader},
    bytecode::{BytecodeHash, BytecodeMarker},
    commitment::PubdataParams,
    ethabi,
    ethabi::{ParamType, Token},
    fee::Fee,
    fee_model::FeeParams,
    l1::L1Tx,
    l2::L2Tx,
    transaction_request::{CallRequest, Eip712Meta},
    tx::{execute::Create2DeploymentParams, IncludedTxLocation},
    u256_to_h256,
    utils::storage_key_for_eth_balance,
    AccountTreeId, Address, Execute, L1BatchNumber, L2BlockNumber, ProtocolVersionId, StorageKey,
    StorageLog, Transaction, EIP_712_TX_TYPE, H256, U256,
};
use zksync_vm_executor::{batch::MainBatchExecutorFactory, interface::BatchExecutorFactory};

use crate::execution_sandbox::testonly::apply_state_overrides;

const MULTICALL3_CONTRACT_PATH: &str =
    "contracts/l2-contracts/zkout/Multicall3.sol/Multicall3.json";

/// Inflates the provided bytecode by appending the specified amount of NOP instructions at the end.
fn inflate_bytecode(bytecode: &mut Vec<u8>, nop_count: usize) {
    bytecode.extend(
        std::iter::repeat_n(
            EncodingModeProduction::nop_encoding().to_be_bytes(),
            nop_count,
        )
        .flatten(),
    );
}

pub(crate) fn default_fee() -> Fee {
    let fee_input = FeeParams::sensible_v1_default().scale(1.0, 1.0);
    let (max_fee_per_gas, gas_per_pubdata_limit) =
        derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::default().into());
    Fee {
        gas_limit: 10_000_000.into(),
        max_fee_per_gas: max_fee_per_gas.into(),
        max_priority_fee_per_gas: 0_u64.into(),
        gas_per_pubdata_limit: gas_per_pubdata_limit.into(),
    }
}

#[derive(Debug, Default)]
pub(crate) struct StateBuilder {
    inner: HashMap<Address, OverrideAccount>,
}

impl StateBuilder {
    pub(crate) const LOAD_TEST_ADDRESS: Address = Address::repeat_byte(1);
    pub(crate) const EXPENSIVE_CONTRACT_ADDRESS: Address = Address::repeat_byte(2);
    pub(crate) const PRECOMPILES_CONTRACT_ADDRESS: Address = Address::repeat_byte(3);
    pub(crate) const COUNTER_CONTRACT_ADDRESS: Address = Address::repeat_byte(4);
    const INFINITE_LOOP_CONTRACT_ADDRESS: Address = Address::repeat_byte(5);
    const MULTICALL3_ADDRESS: Address = Address::repeat_byte(6);

    pub fn with_contract(mut self, address: Address, bytecode: Vec<u8>) -> Self {
        self.inner.insert(
            address,
            OverrideAccount {
                code: Some(BytecodeOverride::Unspecified(bytecode.into())),
                ..OverrideAccount::default()
            },
        );
        self
    }

    pub fn inflate_bytecode(mut self, address: Address, nop_count: usize) -> Self {
        let account_override = self.inner.get_mut(&address).expect("no contract");
        let code_override = account_override.code.as_mut().expect("no code override");
        let BytecodeOverride::Unspecified(code) = code_override else {
            panic!("unexpected bytecode override: {code_override:?}");
        };
        inflate_bytecode(&mut code.0, nop_count);
        self
    }

    pub fn with_load_test_contract(mut self) -> Self {
        let code = TestContract::load_test().bytecode.to_vec();
        // Set the array length in the load test contract to 100, so that reads don't fail.
        let state = HashMap::from([(H256::zero(), H256::from_low_u64_be(100))]);
        self.inner.insert(
            Self::LOAD_TEST_ADDRESS,
            OverrideAccount {
                code: Some(BytecodeOverride::Unspecified(code.into())),
                state: Some(OverrideState::State(state)),
                ..OverrideAccount::default()
            },
        );
        self
    }

    pub fn with_balance(mut self, address: Address, balance: U256) -> Self {
        self.inner.entry(address).or_default().balance = Some(balance);
        self
    }

    pub fn with_nonce(mut self, address: Address, nonce: U256) -> Self {
        self.inner.entry(address).or_default().nonce = Some(nonce);
        self
    }

    pub fn with_storage_slot(mut self, address: Address, slot: H256, value: H256) -> Self {
        let account_entry = self.inner.entry(address).or_default();
        let state = account_entry
            .state
            .get_or_insert_with(|| OverrideState::State(HashMap::new()));
        let state = match state {
            OverrideState::State(state) | OverrideState::StateDiff(state) => state,
        };
        state.insert(slot, value);
        self
    }

    pub fn enable_evm_deployments(self) -> Self {
        let allowed_contract_types_slot = H256::from_low_u64_be(1);
        self.with_storage_slot(
            CONTRACT_DEPLOYER_ADDRESS,
            allowed_contract_types_slot,
            H256::from_low_u64_be(1),
        )
    }

    pub fn with_expensive_contract(self) -> Self {
        self.with_contract(
            Self::EXPENSIVE_CONTRACT_ADDRESS,
            TestContract::expensive().bytecode.to_vec(),
        )
    }

    pub fn with_precompiles_contract(self) -> Self {
        self.with_contract(
            Self::PRECOMPILES_CONTRACT_ADDRESS,
            TestContract::precompiles_test().bytecode.to_vec(),
        )
    }

    pub fn with_counter_contract(self, initial_value: Option<u64>) -> Self {
        self.with_generic_counter_contract(BytecodeMarker::EraVm, initial_value)
    }

    pub fn with_evm_counter_contract(self, initial_value: Option<u64>) -> Self {
        self.with_generic_counter_contract(BytecodeMarker::Evm, initial_value)
    }

    pub(crate) fn with_generic_counter_contract(
        self,
        kind: BytecodeMarker,
        initial_value: Option<u64>,
    ) -> Self {
        let bytecode = match kind {
            BytecodeMarker::EraVm => TestContract::counter().bytecode,
            BytecodeMarker::Evm => TestEvmContract::counter().deployed_bytecode,
        };
        let mut this = self.with_contract(Self::COUNTER_CONTRACT_ADDRESS, bytecode.to_vec());
        if let Some(initial_value) = initial_value {
            let state = HashMap::from([(H256::zero(), H256::from_low_u64_be(initial_value))]);
            this.inner
                .get_mut(&Self::COUNTER_CONTRACT_ADDRESS)
                .unwrap()
                .state = Some(OverrideState::State(state));
        }
        this
    }

    pub fn with_infinite_loop_contract(self) -> Self {
        self.with_contract(
            Self::INFINITE_LOOP_CONTRACT_ADDRESS,
            TestContract::infinite_loop().bytecode.to_vec(),
        )
    }

    pub fn with_multicall3_contract(self) -> Self {
        self.with_contract(
            Self::MULTICALL3_ADDRESS,
            read_bytecode(MULTICALL3_CONTRACT_PATH),
        )
    }

    pub fn build(self) -> StateOverride {
        StateOverride::new(self.inner)
    }

    /// Applies these state overrides to Postgres storage, which is assumed to be empty (other than genesis data).
    pub async fn apply(self, connection: Connection<'static, Core>) {
        apply_state_overrides(connection, StateOverride::new(self.inner)).await;
    }
}

#[derive(Debug)]
pub(crate) struct Call3Value {
    target: Address,
    allow_failure: bool,
    value: U256,
    calldata: Vec<u8>,
}

impl Call3Value {
    pub fn allow_failure(mut self) -> Self {
        self.allow_failure = true;
        self
    }

    fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::Address(self.target),
            Token::Bool(self.allow_failure),
            Token::Uint(self.value),
            Token::Bytes(self.calldata.clone()),
        ])
    }
}

impl From<CallRequest> for Call3Value {
    fn from(req: CallRequest) -> Self {
        Self {
            target: req.to.unwrap(),
            allow_failure: false,
            value: req.value.unwrap_or_default(),
            calldata: req.data.unwrap_or_default().0,
        }
    }
}

impl From<L2Tx> for Call3Value {
    fn from(tx: L2Tx) -> Self {
        Self {
            target: tx.recipient_account().unwrap_or_default(),
            allow_failure: false,
            value: tx.execute.value,
            calldata: tx.execute.calldata,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Call3Result {
    pub success: bool,
    pub return_data: Vec<u8>,
}

impl Call3Result {
    pub fn parse(raw: &[u8]) -> Vec<Self> {
        let mut tokens = load_contract(MULTICALL3_CONTRACT_PATH)
            .function("aggregate3Value")
            .expect("no `aggregate3Value` function")
            .decode_output(raw)
            .expect("failed decoding `aggregate3Value` output");
        assert_eq!(tokens.len(), 1, "Invalid output length");
        let Token::Array(results) = tokens.pop().unwrap() else {
            panic!("Invalid token type, expected an array");
        };
        results.into_iter().map(Self::parse_single).collect()
    }

    fn parse_single(token: Token) -> Self {
        let Token::Tuple(mut tokens) = token else {
            panic!("Invalid token type, expected a tuple");
        };
        assert_eq!(tokens.len(), 2);
        let return_data = tokens.pop().unwrap().into_bytes().expect("expected bytes");
        let success = tokens.pop().unwrap().into_bool().expect("expected bool");
        Self {
            success,
            return_data,
        }
    }

    pub fn as_u256(&self) -> U256 {
        decode_u256_output(&self.return_data)
    }
}

pub(crate) fn decode_u256_output(raw_output: &[u8]) -> U256 {
    let mut tokens = ethabi::decode_whole(&[ethabi::ParamType::Uint(256)], raw_output)
        .expect("unexpected return data");
    assert_eq!(tokens.len(), 1);
    tokens.pop().unwrap().into_uint().unwrap()
}

pub(crate) trait TestAccount {
    fn create_transfer(&mut self, value: U256) -> L2Tx {
        let fee = Fee {
            gas_limit: 200_000.into(),
            ..default_fee()
        };
        self.create_transfer_with_fee(Address::random(), value, fee)
    }

    fn query_base_token_balance(&self) -> CallRequest;

    fn query_min_nonce(&self, address: Address) -> CallRequest;

    fn create_transfer_with_fee(&mut self, to: Address, value: U256, fee: Fee) -> L2Tx;

    fn create_load_test_tx(&mut self, params: LoadnextContractExecutionParams) -> L2Tx;

    fn create_expensive_tx(&mut self, write_count: usize) -> L2Tx;

    fn create_expensive_cleanup_tx(&mut self) -> L2Tx;

    fn create_code_oracle_tx(&mut self, bytecode_hash: H256, expected_keccak_hash: H256) -> L2Tx;

    fn create_counter_tx(&mut self, increment: U256, revert: bool) -> L2Tx;

    fn create_l1_counter_tx(&self, increment: U256, revert: bool) -> L1Tx;

    fn query_counter_value(&self) -> CallRequest;

    fn create_infinite_loop_tx(&mut self) -> L2Tx;

    fn multicall_with_value(&self, value: U256, calls: &[Call3Value]) -> CallRequest;

    fn create2_account(&mut self, bytecode: Vec<u8>) -> (L2Tx, Address);

    fn create_evm_counter_deployment(&mut self, initial_value: U256) -> L2Tx;
}

impl TestAccount for Account {
    fn create_transfer_with_fee(&mut self, to: Address, value: U256, fee: Fee) -> L2Tx {
        let execute = Execute {
            contract_address: Some(to),
            calldata: vec![],
            value,
            factory_deps: vec![],
        };
        self.get_l2_tx_for_execute(execute, Some(fee))
            .try_into()
            .unwrap()
    }

    fn query_base_token_balance(&self) -> CallRequest {
        let data = zksync_contracts::eth_contract()
            .function("balanceOf")
            .expect("No `balanceOf` function in contract")
            .encode_input(&[Token::Uint(address_to_u256(&self.address()))])
            .expect("failed encoding `balanceOf` function");
        CallRequest {
            from: Some(self.address()),
            to: Some(L2_BASE_TOKEN_ADDRESS),
            data: Some(data.into()),
            ..CallRequest::default()
        }
    }

    fn query_min_nonce(&self, address: Address) -> CallRequest {
        let signature = ethabi::short_signature("getMinNonce", &[ParamType::Address]);
        let mut data = signature.to_vec();
        data.extend_from_slice(&ethabi::encode(&[Token::Address(address)]));
        CallRequest {
            from: Some(self.address()),
            to: Some(NONCE_HOLDER_ADDRESS),
            data: Some(data.into()),
            ..CallRequest::default()
        }
    }

    fn create_load_test_tx(&mut self, params: LoadnextContractExecutionParams) -> L2Tx {
        let execute = Execute {
            contract_address: Some(StateBuilder::LOAD_TEST_ADDRESS),
            calldata: params.to_bytes(),
            value: 0.into(),
            factory_deps: if params.deploys > 0 {
                TestContract::load_test().factory_deps()
            } else {
                vec![]
            },
        };
        self.get_l2_tx_for_execute(execute, Some(default_fee()))
            .try_into()
            .unwrap()
    }

    fn create_expensive_tx(&mut self, write_count: usize) -> L2Tx {
        let calldata = TestContract::expensive()
            .function("expensive")
            .encode_input(&[Token::Uint(write_count.into())])
            .expect("failed encoding `expensive` function");
        let execute = Execute {
            contract_address: Some(StateBuilder::EXPENSIVE_CONTRACT_ADDRESS),
            calldata,
            value: 0.into(),
            factory_deps: vec![],
        };
        self.get_l2_tx_for_execute(execute, Some(default_fee()))
            .try_into()
            .unwrap()
    }

    fn create_expensive_cleanup_tx(&mut self) -> L2Tx {
        let calldata = TestContract::expensive()
            .function("cleanUp")
            .encode_input(&[])
            .expect("failed encoding `cleanUp` input");
        let execute = Execute {
            contract_address: Some(StateBuilder::EXPENSIVE_CONTRACT_ADDRESS),
            calldata,
            value: 0.into(),
            factory_deps: vec![],
        };
        self.get_l2_tx_for_execute(execute, Some(default_fee()))
            .try_into()
            .unwrap()
    }

    fn create_code_oracle_tx(&mut self, bytecode_hash: H256, expected_keccak_hash: H256) -> L2Tx {
        let calldata = TestContract::precompiles_test()
            .function("callCodeOracle")
            .encode_input(&[
                Token::FixedBytes(bytecode_hash.0.to_vec()),
                Token::FixedBytes(expected_keccak_hash.0.to_vec()),
            ])
            .expect("failed encoding `callCodeOracle` input");
        let execute = Execute {
            contract_address: Some(StateBuilder::PRECOMPILES_CONTRACT_ADDRESS),
            calldata,
            value: 0.into(),
            factory_deps: vec![],
        };
        self.get_l2_tx_for_execute(execute, Some(default_fee()))
            .try_into()
            .unwrap()
    }

    fn create_counter_tx(&mut self, increment: U256, revert: bool) -> L2Tx {
        let calldata = TestContract::counter()
            .function("incrementWithRevert")
            .encode_input(&[Token::Uint(increment), Token::Bool(revert)])
            .expect("failed encoding `incrementWithRevert` input");
        let execute = Execute {
            contract_address: Some(StateBuilder::COUNTER_CONTRACT_ADDRESS),
            calldata,
            value: 0.into(),
            factory_deps: vec![],
        };
        self.get_l2_tx_for_execute(execute, Some(default_fee()))
            .try_into()
            .unwrap()
    }

    fn create_l1_counter_tx(&self, increment: U256, revert: bool) -> L1Tx {
        let calldata = TestContract::counter()
            .function("incrementWithRevert")
            .encode_input(&[Token::Uint(increment), Token::Bool(revert)])
            .expect("failed encoding `incrementWithRevert` input");
        let request = CallRequest {
            data: Some(calldata.into()),
            from: Some(self.address()),
            to: Some(StateBuilder::COUNTER_CONTRACT_ADDRESS),
            transaction_type: Some(EIP_712_TX_TYPE.into()),
            eip712_meta: Some(Eip712Meta {
                gas_per_pubdata: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
                ..Eip712Meta::default()
            }),
            ..CallRequest::default()
        };
        L1Tx::from_request(request, false).unwrap()
    }

    fn query_counter_value(&self) -> CallRequest {
        let calldata = TestContract::counter()
            .function("get")
            .encode_input(&[])
            .expect("failed encoding `get` input");
        CallRequest {
            from: Some(self.address()),
            to: Some(StateBuilder::COUNTER_CONTRACT_ADDRESS),
            data: Some(calldata.into()),
            ..CallRequest::default()
        }
    }

    fn create_infinite_loop_tx(&mut self) -> L2Tx {
        let calldata = TestContract::infinite_loop()
            .function("infiniteLoop")
            .encode_input(&[])
            .expect("failed encoding `infiniteLoop` input");
        let execute = Execute {
            contract_address: Some(StateBuilder::INFINITE_LOOP_CONTRACT_ADDRESS),
            calldata,
            value: 0.into(),
            factory_deps: vec![],
        };
        self.get_l2_tx_for_execute(execute, Some(default_fee()))
            .try_into()
            .unwrap()
    }

    fn multicall_with_value(&self, value: U256, calls: &[Call3Value]) -> CallRequest {
        let call_tokens = calls.iter().map(Call3Value::to_token).collect();
        let calldata = load_contract(MULTICALL3_CONTRACT_PATH)
            .function("aggregate3Value")
            .expect("no `aggregate3Value` function")
            .encode_input(&[Token::Array(call_tokens)])
            .expect("failed encoding `aggregate3Value` input");
        CallRequest {
            from: Some(self.address()),
            to: Some(StateBuilder::MULTICALL3_ADDRESS),
            value: Some(value),
            data: Some(calldata.into()),
            ..CallRequest::default()
        }
    }

    fn create2_account(&mut self, bytecode: Vec<u8>) -> (L2Tx, Address) {
        let create2_params = Create2DeploymentParams {
            salt: H256::zero(),
            bytecode_hash: BytecodeHash::for_bytecode(&bytecode).value(),
            raw_constructor_input: vec![],
        };
        let deployed_address = create2_params.derive_address(self.address());

        let calldata = zksync_contracts::deployer_contract()
            .function("create2Account")
            .expect("no `create2Account` function")
            .encode_input(&[
                Token::FixedBytes(create2_params.salt.as_bytes().to_vec()),
                Token::FixedBytes(create2_params.bytecode_hash.as_bytes().to_vec()),
                Token::Bytes(create2_params.raw_constructor_input),
                Token::Uint(1.into()), // AA version
            ])
            .expect("failed encoding `create2Account` input");
        let execute = Execute {
            contract_address: Some(CONTRACT_DEPLOYER_ADDRESS),
            calldata,
            value: 0.into(),
            factory_deps: vec![bytecode],
        };
        let deploy_tx = self
            .get_l2_tx_for_execute(execute, Some(default_fee()))
            .try_into()
            .unwrap();
        (deploy_tx, deployed_address)
    }

    fn create_evm_counter_deployment(&mut self, initial_value: U256) -> L2Tx {
        self.get_evm_deploy_tx(
            TestEvmContract::counter().init_bytecode.to_vec(),
            &TestEvmContract::counter().abi,
            &[Token::Uint(initial_value)],
        )
    }
}

pub(crate) fn mock_execute_transaction(transaction: Transaction) -> TransactionExecutionResult {
    TransactionExecutionResult {
        hash: transaction.hash(),
        transaction,
        execution_info: VmExecutionMetrics::default(),
        execution_status: TxExecutionStatus::Success,
        refunded_gas: 0,
        call_traces: vec![],
        revert_reason: None,
    }
}

pub(crate) async fn store_custom_l2_block(
    storage: &mut Connection<'_, Core>,
    header: &L2BlockHeader,
    transaction_results: &[TransactionExecutionResult],
    l1_batch_number: L1BatchNumber,
) -> anyhow::Result<()> {
    let number = header.number;
    for result in transaction_results {
        let l2_tx = result.transaction.clone().try_into().unwrap();
        let tx_submission_result = storage
            .transactions_dal()
            .insert_transaction_l2(
                &l2_tx,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        assert_matches!(tx_submission_result, L2TxSubmissionResult::Added);
    }

    // Record L2 block info which is read by the VM sandbox logic
    let l2_block_info_key = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    );
    let block_info = pack_block_info(number.0.into(), number.0.into());
    let l2_block_log = StorageLog::new_write_log(l2_block_info_key, u256_to_h256(block_info));
    storage
        .storage_logs_dal()
        .append_storage_logs(number, &[l2_block_log])
        .await?;

    storage
        .blocks_dal()
        .insert_l2_block(header, l1_batch_number)
        .await?;
    storage
        .transactions_dal()
        .mark_txs_as_executed_in_l2_block(
            number,
            transaction_results,
            1.into(),
            ProtocolVersionId::latest(),
            false,
        )
        .await?;
    Ok(())
}

/// Executes transactions and stores real events and storage logs.
pub(crate) async fn persist_block_with_transactions(
    pool: &ConnectionPool<Core>,
    txs: Vec<Transaction>,
) {
    let mut storage = pool.connection().await.unwrap();
    let prev_block = storage
        .blocks_dal()
        .get_last_sealed_l2_block_header()
        .await
        .unwrap()
        .expect("no blocks in storage");
    assert_eq!(prev_block.number, L2BlockNumber(0));
    let block_header = create_l2_block(1);
    let block_number = block_header.number;

    let system_env = default_system_env();
    let mut l1_batch_env = default_l1_batch_env(1, 1, Address::repeat_byte(1));
    l1_batch_env.first_l2_block.prev_block_hash = prev_block.hash;
    l1_batch_env.previous_batch_hash = Some(
        storage
            .blocks_dal()
            .get_l1_batch_state_root(L1BatchNumber(0))
            .await
            .unwrap()
            .expect("no root hash for genesis L1 batch"),
    );

    let executor_storage = PostgresStorage::new_async(
        tokio::runtime::Handle::current(),
        pool.connection().await.unwrap(),
        block_number - 1,
        false,
    )
    .await
    .unwrap();

    let mut batch_executor = MainBatchExecutorFactory::<()>::new(false).init_batch(
        executor_storage,
        l1_batch_env,
        system_env,
        PubdataParams::default(),
    );

    let mut all_events = vec![];
    let mut events_by_transaction = vec![];
    let mut all_logs = vec![];
    let mut all_tx_results = vec![];
    for (i, tx) in txs.into_iter().enumerate() {
        let tx_result = batch_executor
            .execute_tx(tx.clone())
            .await
            .unwrap()
            .tx_result;

        let start_idx = all_events.len();
        all_events.extend(tx_result.logs.events);
        let tx_location = IncludedTxLocation {
            tx_hash: tx.hash(),
            tx_index_in_l2_block: i as u32,
        };
        events_by_transaction.push((tx_location, start_idx..all_events.len()));
        all_logs.extend(tx_result.logs.storage_logs);
        all_tx_results.push(TransactionExecutionResult {
            execution_status: match tx_result.result {
                ExecutionResult::Success { .. } => TxExecutionStatus::Success,
                ExecutionResult::Revert { .. } => TxExecutionStatus::Failure,
                other => panic!("unexpected tx result: {other:?}"),
            },
            ..mock_execute_transaction(tx)
        });
    }

    let events_by_transaction: Vec<_> = events_by_transaction
        .into_iter()
        .map(|(location, range)| (location, all_events[range].iter().collect::<Vec<_>>()))
        .collect();

    storage
        .events_dal()
        .save_events(block_number, &events_by_transaction)
        .await
        .unwrap();
    let deduplicated_logs = StorageWritesDeduplicator::deduplicate_logs(all_logs.iter());
    storage
        .storage_logs_dal()
        .insert_storage_logs(block_number, &deduplicated_logs)
        .await
        .unwrap();
    store_custom_l2_block(
        &mut storage,
        &block_header,
        &all_tx_results,
        L1BatchNumber(1),
    )
    .await
    .unwrap();
}

#[cfg(test)]
mod tests {
    use zksync_test_contracts::TxType;

    use super::*;

    #[test]
    fn bytecode_kind_is_correctly_detected_for_test_contracts() {
        let era_contracts = [
            TestContract::counter(),
            TestContract::load_test(),
            TestContract::infinite_loop(),
            TestContract::expensive(),
            TestContract::precompiles_test(),
        ];
        for contract in era_contracts {
            assert_eq!(
                BytecodeMarker::detect(contract.bytecode),
                BytecodeMarker::EraVm
            );
        }

        let evm_contracts = [TestEvmContract::counter(), TestEvmContract::evm_tester()];
        for contract in evm_contracts {
            assert_eq!(
                BytecodeMarker::detect(contract.deployed_bytecode),
                BytecodeMarker::Evm
            );
        }
    }

    #[tokio::test]
    async fn persisting_block_with_transactions_works() {
        let mut alice = Account::random();
        let transfer = alice.create_transfer(1.into());
        let transfer_hash = transfer.hash();
        let deployment = alice
            .get_deploy_tx(TestContract::counter().bytecode, None, TxType::L2)
            .tx;
        let deployment_hash = deployment.hash();

        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        let balance_key = storage_key_for_eth_balance(&alice.address());
        let balance_log = StorageLog::new_write_log(balance_key, H256::from_low_u64_be(u64::MAX));
        storage
            .storage_logs_dal()
            .append_storage_logs(L2BlockNumber(0), &[balance_log])
            .await
            .unwrap();

        persist_block_with_transactions(&pool, vec![transfer.into(), deployment]).await;

        let mut receipts = storage
            .transactions_web3_dal()
            .get_transaction_receipts(&[transfer_hash, deployment_hash])
            .await
            .unwrap();
        assert_eq!(receipts.len(), 2);
        receipts.sort_unstable_by_key(|receipt| receipt.inner.transaction_index);

        assert_eq!(receipts[0].inner.from, alice.address());
        assert_eq!(receipts[0].inner.transaction_hash, transfer_hash);
        assert_eq!(receipts[0].inner.status, 1.into());
        assert_eq!(receipts[0].nonce, 0.into());
        assert_eq!(receipts[0].calldata.0, [] as [u8; 0]);

        assert_eq!(receipts[1].inner.from, alice.address());
        assert_eq!(receipts[1].inner.transaction_hash, deployment_hash);
        assert_eq!(receipts[1].inner.status, 1.into());
        assert_eq!(receipts[1].nonce, 1.into());
        assert!(!receipts[1].calldata.0.is_empty());

        // Check that the transactions have storage logs and events persisted
        let new_storage_logs: Vec<_> = storage
            .storage_logs_dal()
            .dump_all_storage_logs_for_tests()
            .await
            .into_iter()
            .filter(|log| log.l2_block_number == L2BlockNumber(1))
            .collect();
        assert!(!new_storage_logs.is_empty());
        assert!(
            new_storage_logs
                .iter()
                .any(|log| log.hashed_key == balance_key.hashed_key()),
            "{new_storage_logs:#?}"
        );

        let new_events = storage
            .events_web3_dal()
            .get_all_logs(L2BlockNumber(0))
            .await
            .unwrap();
        assert!(!new_events.is_empty());
    }
}
