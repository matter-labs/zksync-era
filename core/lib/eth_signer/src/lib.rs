use async_trait::async_trait;
use zksync_basic_types::Address;
use zksync_crypto_primitives::{EIP712TypedStructure, Eip712Domain, PackedEthSignature};

pub use crate::{pk_signer::PrivateKeySigner, raw_ethereum_tx::TransactionParameters};

mod pk_signer;
mod raw_ethereum_tx;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SignerError {
    #[error("Signing failed: {0}")]
    SigningFailed(String),
}

#[async_trait]
pub trait EthereumSigner: 'static + Send + Sync + Clone {
    async fn sign_typed_data<S: EIP712TypedStructure + Sync>(
        &self,
        domain: &Eip712Domain,
        typed_struct: &S,
    ) -> Result<PackedEthSignature, SignerError>;

    async fn sign_transaction(&self, raw_tx: TransactionParameters)
        -> Result<Vec<u8>, SignerError>;

    async fn get_address(&self) -> Result<Address, SignerError>;
}
