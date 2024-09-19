//! Test utils shared among multiple modules.

use zksync_multivm::zk_evm_latest::ethereum_types::{Address, U256};
use zksync_types::{
    fee::Fee, l2::L2Tx, transaction_request::PaymasterParams, K256PrivateKey, L2ChainId, Nonce,
};

/// Creates a correctly signed transfer transaction.
pub(crate) fn create_transfer(value: U256, fee_per_gas: u64, gas_per_pubdata: u64) -> L2Tx {
    let fee = Fee {
        gas_limit: 200_000.into(),
        max_fee_per_gas: fee_per_gas.into(),
        max_priority_fee_per_gas: 0_u64.into(),
        gas_per_pubdata_limit: gas_per_pubdata.into(),
    };
    L2Tx::new_signed(
        Some(Address::random()),
        vec![],
        Nonce(0),
        fee,
        value,
        L2ChainId::default(),
        &K256PrivateKey::random(),
        vec![],
        PaymasterParams::default(),
    )
    .unwrap()
}
