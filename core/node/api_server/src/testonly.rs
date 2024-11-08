//! Test utils shared among multiple modules.

use std::{collections::HashMap, iter};

use zk_evm_1_5_0::zkevm_opcode_defs::decoding::{EncodingModeProduction, VmEncodingMode};
use zksync_contracts::{
    eth_contract, get_loadnext_contract, load_contract, read_bytecode,
    test_contracts::LoadnextContractExecutionParams,
};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_multivm::utils::derive_base_fee_and_gas_per_pubdata;
use zksync_system_constants::{L2_BASE_TOKEN_ADDRESS, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE};
use zksync_types::{
    api::state_override::{Bytecode, OverrideAccount, OverrideState, StateOverride},
    ethabi,
    ethabi::Token,
    fee::Fee,
    fee_model::FeeParams,
    get_code_key, get_known_code_key,
    l1::L1Tx,
    l2::L2Tx,
    transaction_request::{CallRequest, Eip712Meta, PaymasterParams},
    utils::storage_key_for_eth_balance,
    AccountTreeId, Address, K256PrivateKey, L2BlockNumber, L2ChainId, Nonce, ProtocolVersionId,
    StorageKey, StorageLog, EIP_712_TX_TYPE, H256, U256,
};
use zksync_utils::{address_to_u256, u256_to_h256};

const EXPENSIVE_CONTRACT_PATH: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/expensive/expensive.sol/Expensive.json";
const PRECOMPILES_CONTRACT_PATH: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/precompiles/precompiles.sol/Precompiles.json";
const COUNTER_CONTRACT_PATH: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json";
const INFINITE_LOOP_CONTRACT_PATH: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/infinite/infinite.sol/InfiniteLoop.json";
const MULTICALL3_CONTRACT_PATH: &str =
    "contracts/l2-contracts/zkout/Multicall3.sol/Multicall3.json";

/// Inflates the provided bytecode by appending the specified amount of NOP instructions at the end.
fn inflate_bytecode(bytecode: &mut Vec<u8>, nop_count: usize) {
    bytecode.extend(
        iter::repeat(EncodingModeProduction::nop_encoding().to_be_bytes())
            .take(nop_count)
            .flatten(),
    );
}

fn default_fee() -> Fee {
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
    const COUNTER_CONTRACT_ADDRESS: Address = Address::repeat_byte(4);
    const INFINITE_LOOP_CONTRACT_ADDRESS: Address = Address::repeat_byte(5);
    const MULTICALL3_ADDRESS: Address = Address::repeat_byte(6);

    pub fn with_contract(mut self, address: Address, bytecode: Vec<u8>) -> Self {
        self.inner.insert(
            address,
            OverrideAccount {
                code: Some(Bytecode::new(bytecode).unwrap()),
                ..OverrideAccount::default()
            },
        );
        self
    }

    pub fn inflate_bytecode(mut self, address: Address, nop_count: usize) -> Self {
        let account_override = self.inner.get_mut(&address).expect("no contract");
        let bytecode = account_override.code.take().expect("no code override");
        let mut bytecode = bytecode.into_bytes();
        inflate_bytecode(&mut bytecode, nop_count);
        account_override.code = Some(Bytecode::new(bytecode).unwrap());
        self
    }

    pub fn with_load_test_contract(mut self) -> Self {
        // Set the array length in the load test contract to 100, so that reads don't fail.
        let state = HashMap::from([(H256::zero(), H256::from_low_u64_be(100))]);
        self.inner.insert(
            Self::LOAD_TEST_ADDRESS,
            OverrideAccount {
                code: Some(Bytecode::new(get_loadnext_contract().bytecode).unwrap()),
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

    pub fn with_expensive_contract(self) -> Self {
        self.with_contract(
            Self::EXPENSIVE_CONTRACT_ADDRESS,
            read_bytecode(EXPENSIVE_CONTRACT_PATH),
        )
    }

    pub fn with_precompiles_contract(self) -> Self {
        self.with_contract(
            Self::PRECOMPILES_CONTRACT_ADDRESS,
            read_bytecode(PRECOMPILES_CONTRACT_PATH),
        )
    }

    pub fn with_counter_contract(self, initial_value: u64) -> Self {
        let mut this = self.with_contract(
            Self::COUNTER_CONTRACT_ADDRESS,
            read_bytecode(COUNTER_CONTRACT_PATH),
        );
        if initial_value != 0 {
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
            read_bytecode(INFINITE_LOOP_CONTRACT_PATH),
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
    pub async fn apply(self, connection: &mut Connection<'_, Core>) {
        let mut storage_logs = vec![];
        let mut factory_deps = HashMap::new();
        for (address, account) in self.inner {
            if let Some(balance) = account.balance {
                let balance_key = storage_key_for_eth_balance(&address);
                storage_logs.push(StorageLog::new_write_log(
                    balance_key,
                    u256_to_h256(balance),
                ));
            }
            if let Some(code) = account.code {
                let code_hash = code.hash();
                storage_logs.extend([
                    StorageLog::new_write_log(get_code_key(&address), code_hash),
                    StorageLog::new_write_log(
                        get_known_code_key(&code_hash),
                        H256::from_low_u64_be(1),
                    ),
                ]);
                factory_deps.insert(code_hash, code.into_bytes());
            }
            if let Some(state) = account.state {
                let state_slots = match state {
                    OverrideState::State(slots) | OverrideState::StateDiff(slots) => slots,
                };
                let state_logs = state_slots.into_iter().map(|(key, value)| {
                    let key = StorageKey::new(AccountTreeId::new(address), key);
                    StorageLog::new_write_log(key, value)
                });
                storage_logs.extend(state_logs);
            }
        }

        connection
            .storage_logs_dal()
            .append_storage_logs(L2BlockNumber(0), &storage_logs)
            .await
            .unwrap();
        connection
            .factory_deps_dal()
            .insert_factory_deps(L2BlockNumber(0), &factory_deps)
            .await
            .unwrap();
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
            target: tx.recipient_account().unwrap(),
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
    fn create_transfer(&self, value: U256) -> L2Tx {
        let fee = Fee {
            gas_limit: 200_000.into(),
            ..default_fee()
        };
        self.create_transfer_with_fee(value, fee)
    }

    fn query_base_token_balance(&self) -> CallRequest;

    fn create_transfer_with_fee(&self, value: U256, fee: Fee) -> L2Tx;

    fn create_load_test_tx(&self, params: LoadnextContractExecutionParams) -> L2Tx;

    fn create_expensive_tx(&self, write_count: usize) -> L2Tx;

    fn create_expensive_cleanup_tx(&self) -> L2Tx;

    fn create_code_oracle_tx(&self, bytecode_hash: H256, expected_keccak_hash: H256) -> L2Tx;

    fn create_counter_tx(&self, increment: U256, revert: bool) -> L2Tx;

    fn create_l1_counter_tx(&self, increment: U256, revert: bool) -> L1Tx;

    fn query_counter_value(&self) -> CallRequest;

    fn create_infinite_loop_tx(&self) -> L2Tx;

    fn multicall_with_value(&self, value: U256, calls: &[Call3Value]) -> CallRequest;
}

impl TestAccount for K256PrivateKey {
    fn create_transfer_with_fee(&self, value: U256, fee: Fee) -> L2Tx {
        L2Tx::new_signed(
            Some(Address::random()),
            vec![],
            Nonce(0),
            fee,
            value,
            L2ChainId::default(),
            self,
            vec![],
            PaymasterParams::default(),
        )
        .unwrap()
    }

    fn query_base_token_balance(&self) -> CallRequest {
        let data = eth_contract()
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

    fn create_load_test_tx(&self, params: LoadnextContractExecutionParams) -> L2Tx {
        L2Tx::new_signed(
            Some(StateBuilder::LOAD_TEST_ADDRESS),
            params.to_bytes(),
            Nonce(0),
            default_fee(),
            0.into(),
            L2ChainId::default(),
            self,
            if params.deploys > 0 {
                get_loadnext_contract().factory_deps
            } else {
                vec![]
            },
            PaymasterParams::default(),
        )
        .unwrap()
    }

    fn create_expensive_tx(&self, write_count: usize) -> L2Tx {
        let calldata = load_contract(EXPENSIVE_CONTRACT_PATH)
            .function("expensive")
            .expect("no `expensive` function in contract")
            .encode_input(&[Token::Uint(write_count.into())])
            .expect("failed encoding `expensive` function");
        L2Tx::new_signed(
            Some(StateBuilder::EXPENSIVE_CONTRACT_ADDRESS),
            calldata,
            Nonce(0),
            default_fee(),
            0.into(),
            L2ChainId::default(),
            self,
            vec![],
            PaymasterParams::default(),
        )
        .unwrap()
    }

    fn create_expensive_cleanup_tx(&self) -> L2Tx {
        let calldata = load_contract(EXPENSIVE_CONTRACT_PATH)
            .function("cleanUp")
            .expect("no `cleanUp` function in contract")
            .encode_input(&[])
            .expect("failed encoding `cleanUp` input");
        L2Tx::new_signed(
            Some(StateBuilder::EXPENSIVE_CONTRACT_ADDRESS),
            calldata,
            Nonce(0),
            default_fee(),
            0.into(),
            L2ChainId::default(),
            self,
            vec![],
            PaymasterParams::default(),
        )
        .unwrap()
    }

    fn create_code_oracle_tx(&self, bytecode_hash: H256, expected_keccak_hash: H256) -> L2Tx {
        let calldata = load_contract(PRECOMPILES_CONTRACT_PATH)
            .function("callCodeOracle")
            .expect("no `callCodeOracle` function")
            .encode_input(&[
                Token::FixedBytes(bytecode_hash.0.to_vec()),
                Token::FixedBytes(expected_keccak_hash.0.to_vec()),
            ])
            .expect("failed encoding `callCodeOracle` input");
        L2Tx::new_signed(
            Some(StateBuilder::PRECOMPILES_CONTRACT_ADDRESS),
            calldata,
            Nonce(0),
            default_fee(),
            0.into(),
            L2ChainId::default(),
            self,
            vec![],
            PaymasterParams::default(),
        )
        .unwrap()
    }

    fn create_counter_tx(&self, increment: U256, revert: bool) -> L2Tx {
        let calldata = load_contract(COUNTER_CONTRACT_PATH)
            .function("incrementWithRevert")
            .expect("no `incrementWithRevert` function")
            .encode_input(&[Token::Uint(increment), Token::Bool(revert)])
            .expect("failed encoding `incrementWithRevert` input");
        L2Tx::new_signed(
            Some(StateBuilder::COUNTER_CONTRACT_ADDRESS),
            calldata,
            Nonce(0),
            default_fee(),
            0.into(),
            L2ChainId::default(),
            self,
            vec![],
            PaymasterParams::default(),
        )
        .unwrap()
    }

    fn create_l1_counter_tx(&self, increment: U256, revert: bool) -> L1Tx {
        let calldata = load_contract(COUNTER_CONTRACT_PATH)
            .function("incrementWithRevert")
            .expect("no `incrementWithRevert` function")
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
        let calldata = load_contract(COUNTER_CONTRACT_PATH)
            .function("get")
            .expect("no `get` function")
            .encode_input(&[])
            .expect("failed encoding `get` input");
        CallRequest {
            from: Some(self.address()),
            to: Some(StateBuilder::COUNTER_CONTRACT_ADDRESS),
            data: Some(calldata.into()),
            ..CallRequest::default()
        }
    }

    fn create_infinite_loop_tx(&self) -> L2Tx {
        let calldata = load_contract(INFINITE_LOOP_CONTRACT_PATH)
            .function("infiniteLoop")
            .expect("no `infiniteLoop` function")
            .encode_input(&[])
            .expect("failed encoding `infiniteLoop` input");
        L2Tx::new_signed(
            Some(StateBuilder::INFINITE_LOOP_CONTRACT_ADDRESS),
            calldata,
            Nonce(0),
            default_fee(),
            0.into(),
            L2ChainId::default(),
            self,
            vec![],
            PaymasterParams::default(),
        )
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
}
