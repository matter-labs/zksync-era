//! Test utils shared among multiple modules.

use std::iter;

use zk_evm_1_5_0::zkevm_opcode_defs::decoding::{EncodingModeProduction, VmEncodingMode};
use zksync_contracts::{
    get_loadnext_contract, load_contract, read_bytecode,
    test_contracts::LoadnextContractExecutionParams,
};
use zksync_types::{
    ethabi::Token, fee::Fee, l2::L2Tx, transaction_request::PaymasterParams, Address,
    K256PrivateKey, L2ChainId, Nonce, H256, U256,
};

pub(crate) const LOAD_TEST_ADDRESS: Address = Address::repeat_byte(1);

const EXPENSIVE_CONTRACT_PATH: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/expensive/expensive.sol/Expensive.json";
pub(crate) const EXPENSIVE_CONTRACT_ADDRESS: Address = Address::repeat_byte(2);

const PRECOMPILES_CONTRACT_PATH: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/precompiles/precompiles.sol/Precompiles.json";
pub(crate) const PRECOMPILES_CONTRACT_ADDRESS: Address = Address::repeat_byte(3);

const COUNTER_CONTRACT_PATH: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json";
pub(crate) const COUNTER_CONTRACT_ADDRESS: Address = Address::repeat_byte(4);

const INFINITE_LOOP_CONTRACT_PATH: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/infinite/infinite.sol/InfiniteLoop.json";
pub(crate) const INFINITE_LOOP_CONTRACT_ADDRESS: Address = Address::repeat_byte(5);

pub(crate) fn read_expensive_contract_bytecode() -> Vec<u8> {
    read_bytecode(EXPENSIVE_CONTRACT_PATH)
}

pub(crate) fn read_precompiles_contract_bytecode() -> Vec<u8> {
    read_bytecode(PRECOMPILES_CONTRACT_PATH)
}

pub(crate) fn read_counter_contract_bytecode() -> Vec<u8> {
    read_bytecode(COUNTER_CONTRACT_PATH)
}

pub(crate) fn read_infinite_loop_contract_bytecode() -> Vec<u8> {
    read_bytecode(INFINITE_LOOP_CONTRACT_PATH)
}

/// Inflates the provided bytecode by appending the specified amount of NOP instructions at the end.
pub(crate) fn inflate_bytecode(bytecode: &mut Vec<u8>, nop_count: usize) {
    bytecode.extend(
        iter::repeat(EncodingModeProduction::nop_encoding().to_be_bytes())
            .take(nop_count)
            .flatten(),
    );
}

fn default_fee() -> Fee {
    Fee {
        gas_limit: 200_000.into(),
        max_fee_per_gas: 55.into(),
        max_priority_fee_per_gas: 0_u64.into(),
        gas_per_pubdata_limit: 555.into(),
    }
}

pub(crate) trait TestAccount {
    fn create_transfer(&self, value: U256, fee_per_gas: u64, gas_per_pubdata: u64) -> L2Tx {
        let fee = Fee {
            gas_limit: 200_000.into(),
            max_fee_per_gas: fee_per_gas.into(),
            max_priority_fee_per_gas: 0_u64.into(),
            gas_per_pubdata_limit: gas_per_pubdata.into(),
        };
        self.create_transfer_with_fee(value, fee)
    }

    fn create_transfer_with_fee(&self, value: U256, fee: Fee) -> L2Tx;

    fn create_load_test_tx(&self, params: LoadnextContractExecutionParams) -> L2Tx;

    fn create_expensive_tx(&self, write_count: usize) -> L2Tx;

    fn create_expensive_cleanup_tx(&self) -> L2Tx;

    fn create_code_oracle_tx(&self, bytecode_hash: H256, expected_keccak_hash: H256) -> L2Tx;

    fn create_reverting_counter_tx(&self) -> L2Tx;

    fn create_infinite_loop_tx(&self) -> L2Tx;
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

    fn create_load_test_tx(&self, params: LoadnextContractExecutionParams) -> L2Tx {
        L2Tx::new_signed(
            Some(LOAD_TEST_ADDRESS),
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
            Some(EXPENSIVE_CONTRACT_ADDRESS),
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
            Some(EXPENSIVE_CONTRACT_ADDRESS),
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
            Some(PRECOMPILES_CONTRACT_ADDRESS),
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

    fn create_reverting_counter_tx(&self) -> L2Tx {
        let calldata = load_contract(COUNTER_CONTRACT_PATH)
            .function("incrementWithRevert")
            .expect("no `incrementWithRevert` function")
            .encode_input(&[Token::Uint(1.into()), Token::Bool(true)])
            .expect("failed encoding `incrementWithRevert` input");
        L2Tx::new_signed(
            Some(COUNTER_CONTRACT_ADDRESS),
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

    fn create_infinite_loop_tx(&self) -> L2Tx {
        let calldata = load_contract(INFINITE_LOOP_CONTRACT_PATH)
            .function("infiniteLoop")
            .expect("no `infiniteLoop` function")
            .encode_input(&[])
            .expect("failed encoding `infiniteLoop` input");
        L2Tx::new_signed(
            Some(INFINITE_LOOP_CONTRACT_ADDRESS),
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
}
