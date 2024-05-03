//! Various Ethereum client implementations.

mod generic;
mod http;
mod mock;

pub use self::{
    http::{PKSigningClient, QueryClient, SigningClient},
    mock::MockEthereum,
};
