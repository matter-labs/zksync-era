//! Test utils shared among multiple modules.

use zksync_contracts::{get_loadnext_contract, test_contracts::LoadnextContractExecutionParams};
use zksync_types::{
    fee::Fee, l2::L2Tx, transaction_request::PaymasterParams, Address, K256PrivateKey, L2ChainId,
    Nonce, U256,
};

pub(crate) const LOAD_TEST_ADDRESS: Address = Address::repeat_byte(1);

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
        let fee = Fee {
            gas_limit: 200_000.into(),
            max_fee_per_gas: 55.into(),
            max_priority_fee_per_gas: 0_u64.into(),
            gas_per_pubdata_limit: 555.into(),
        };

        L2Tx::new_signed(
            Some(LOAD_TEST_ADDRESS),
            params.to_bytes(),
            Nonce(0),
            fee,
            0.into(),
            L2ChainId::default(),
            self,
            get_loadnext_contract().factory_deps,
            PaymasterParams::default(),
        )
        .unwrap()
    }
}
