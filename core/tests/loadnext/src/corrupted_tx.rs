use async_trait::async_trait;

use zksync::signer::Signer;
use zksync_eth_signer::{error::SignerError, EthereumSigner};
use zksync_types::{Address, EIP712TypedStructure, Eip712Domain, PackedEthSignature, H256};

use crate::command::IncorrectnessModifier;
use zksync_eth_signer::raw_ethereum_tx::TransactionParameters;
use zksync_types::fee::Fee;
use zksync_types::l2::L2Tx;

/// Trait that exists solely to extend the signed zkSync transaction interface, providing the ability
/// to modify transaction in a way that will make it invalid.
///
/// Loadtest is expected to simulate the user behavior, and it's not that uncommon of users to send incorrect
/// transactions.
#[async_trait]
pub trait Corrupted<S: EthereumSigner>: Sized {
    /// Creates a transaction without fee provided.
    async fn zero_fee(self, signer: &Signer<S>) -> Self;

    /// Resigns the transaction after the modification in order to make signatures correct (if applicable).
    async fn resign(&mut self, signer: &Signer<S>);

    /// Automatically chooses one of the methods of this trait based on the provided incorrectness modifier.
    async fn apply_modifier(self, modifier: IncorrectnessModifier, signer: &Signer<S>) -> Self {
        match modifier {
            IncorrectnessModifier::None => self,
            IncorrectnessModifier::IncorrectSignature => self, // signature will be changed before submitting transaction
            IncorrectnessModifier::ZeroFee => self.zero_fee(signer).await,
        }
    }
}

#[async_trait]
impl<S> Corrupted<S> for L2Tx
where
    S: EthereumSigner,
{
    async fn resign(&mut self, signer: &Signer<S>) {
        let signature = signer.sign_transaction(&*self).await.unwrap();
        self.set_signature(signature);
    }

    async fn zero_fee(mut self, signer: &Signer<S>) -> Self {
        self.common_data.fee = Fee::default();
        self.resign(signer).await;
        self
    }
}

#[derive(Debug, Clone)]
pub struct CorruptedSigner {
    address: Address,
}

impl CorruptedSigner {
    fn bad_signature() -> PackedEthSignature {
        let private_key = H256::random();
        let message = b"bad message";
        PackedEthSignature::sign(&private_key, message).unwrap()
    }

    pub fn new(address: Address) -> Self {
        Self { address }
    }
}

#[async_trait]
impl EthereumSigner for CorruptedSigner {
    async fn sign_message(&self, _message: &[u8]) -> Result<PackedEthSignature, SignerError> {
        Ok(Self::bad_signature())
    }

    async fn sign_typed_data<S: EIP712TypedStructure + Sync>(
        &self,
        _domain: &Eip712Domain,
        _typed_struct: &S,
    ) -> Result<PackedEthSignature, SignerError> {
        Ok(Self::bad_signature())
    }

    async fn sign_transaction(
        &self,
        _raw_tx: TransactionParameters,
    ) -> Result<Vec<u8>, SignerError> {
        Ok(b"bad bytes".to_vec())
    }

    async fn get_address(&self) -> Result<Address, SignerError> {
        Ok(self.address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zksync_eth_signer::PrivateKeySigner;
    use zksync_types::fee::Fee;
    use zksync_types::L2ChainId;
    use zksync_types::{
        tokens::ETHEREUM_ADDRESS, tx::primitives::PackedEthSignature, Address, Nonce, H256,
    };

    const AMOUNT: u64 = 100;
    const FEE: u64 = 100;
    const NONCE: Nonce = Nonce(1);

    fn get_signer(chain_id: L2ChainId) -> Signer<PrivateKeySigner> {
        let eth_pk = H256::random();
        let eth_signer = PrivateKeySigner::new(eth_pk);
        let address = PackedEthSignature::address_from_private_key(&eth_pk)
            .expect("Can't get an address from the private key");
        Signer::new(eth_signer, address, chain_id)
    }

    async fn create_transfer<S: EthereumSigner>(signer: &Signer<S>) -> L2Tx {
        let fee = Fee {
            gas_limit: FEE.into(),
            max_fee_per_gas: Default::default(),
            max_priority_fee_per_gas: Default::default(),
            gas_per_pubdata_limit: Default::default(),
        };
        signer
            .sign_transfer(
                Address::repeat_byte(0x7e),
                ETHEREUM_ADDRESS,
                AMOUNT.into(),
                fee,
                NONCE,
                Default::default(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn zero_fee() {
        let chain_id = L2ChainId::default();
        let signer = get_signer(chain_id);

        let transfer = create_transfer(&signer).await;

        let modified_transfer = transfer.zero_fee(&signer).await;

        assert_eq!(modified_transfer.common_data.fee.gas_limit, 0u64.into());
    }
}
