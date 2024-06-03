//! Various Ethereum client implementations.

mod http;
mod mock;

pub use zksync_web3_decl::client::{Client, DynClient, L1};

pub use self::{
    http::{PKSigningClient, SigningClient},
    mock::{MockEthereum, MockEthereumBuilder},
};
