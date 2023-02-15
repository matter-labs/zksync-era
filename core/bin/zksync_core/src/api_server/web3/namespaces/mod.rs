//! Actual implementation of Web3 API namespaces logic, not tied to the backend
//! used to create a JSON RPC server.

pub mod eth;
pub mod eth_subscribe;
pub mod net;
pub mod web3;
pub mod zks;

use num::{rational::Ratio, BigUint};
use zksync_types::U256;
use zksync_utils::{biguint_to_u256, u256_to_biguint};

pub use self::{
    eth::EthNamespace, eth_subscribe::EthSubscribe, net::NetNamespace, web3::Web3Namespace,
    zks::ZksNamespace,
};

pub fn scale_u256(val: U256, scale_factor: &Ratio<BigUint>) -> U256 {
    let val_as_ratio = &Ratio::from_integer(u256_to_biguint(val));
    let result = (val_as_ratio * scale_factor).ceil();

    biguint_to_u256(result.to_integer())
}
