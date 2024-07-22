//! Various Ethereum client implementations.

mod http;
mod mock;

pub use zksync_web3_decl::client::{Client, DynClient, L1};

use serde::{Deserialize, Serialize};
use zksync_types::U256;

pub use self::{
    http::{PKSigningClient, SigningClient},
    mock::{MockEthereum, MockEthereumBuilder},
};

#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineaEstimateGas {
    pub base_fee_per_gas: U256,
    pub gas_limit: U256,
    pub priority_fee_per_gas: U256,
}
