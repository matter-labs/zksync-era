//! Various Ethereum client implementations.

mod http;
mod mock;

pub use zksync_web3_decl::client::{Client, DynClient, ForEthereumLikeNetwork, L1, L2};

pub use self::{
    http::{PKSigningClient, SigningClient},
    mock::{MockEthereumBuilder, MockSettlementLayer},
};
