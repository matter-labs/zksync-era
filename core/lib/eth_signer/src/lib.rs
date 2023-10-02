use async_trait::async_trait;
use error::SignerError;
use zksync_types::tx::primitives::PackedEthSignature;
use zksync_types::{Address, EIP712TypedStructure, Eip712Domain};

pub use crate::raw_ethereum_tx::TransactionParameters;
pub use json_rpc_signer::JsonRpcSigner;
pub use pk_signer::PrivateKeySigner;

pub mod error;
pub mod json_rpc_signer;
pub mod pk_signer;
pub mod raw_ethereum_tx;

#[async_trait]
pub trait EthereumSigner: Send + Sync + Clone {
    async fn sign_message(&self, message: &[u8]) -> Result<PackedEthSignature, SignerError>;
    async fn sign_typed_data<S: EIP712TypedStructure + Sync>(
        &self,
        domain: &Eip712Domain,
        typed_struct: &S,
    ) -> Result<PackedEthSignature, SignerError>;
    async fn sign_transaction(&self, raw_tx: TransactionParameters)
        -> Result<Vec<u8>, SignerError>;
    async fn get_address(&self) -> Result<Address, SignerError>;
}
