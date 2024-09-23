//! Test utils shared among multiple modules.

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

pub(crate) fn read_expensive_contract_bytecode() -> Vec<u8> {
    read_bytecode(EXPENSIVE_CONTRACT_PATH)
}

pub(crate) fn read_precompiles_contract_bytecode() -> Vec<u8> {
    read_bytecode(PRECOMPILES_CONTRACT_PATH)
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

    fn create_decommitting_tx(&self, bytecode_hash: H256, expected_keccak_hash: H256) -> L2Tx;
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
            get_loadnext_contract().factory_deps,
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
            get_loadnext_contract().factory_deps,
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
            get_loadnext_contract().factory_deps,
            PaymasterParams::default(),
        )
        .unwrap()
    }

    fn create_decommitting_tx(&self, bytecode_hash: H256, expected_keccak_hash: H256) -> L2Tx {
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
            get_loadnext_contract().factory_deps,
            PaymasterParams::default(),
        )
        .unwrap()
    }
}
