//! Various Ethereum client implementations.

mod generic;
mod http;
mod mock;

use serde::{Deserialize, Serialize};
use zksync_types::H256;
pub use self::{
    http::{PKSigningClient, QueryClient, SigningClient},
    mock::MockEthereum,
};

#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub struct LineaEstimateGas {
    pub base_fee_per_gas: H256,
    pub gas_limit: H256,
    pub priority_fee_per_gas: H256,
}