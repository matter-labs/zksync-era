use async_trait::async_trait;
use zksync_basic_types::Address;
use zksync_crypto_primitives::{
    EIP712TypedStructure, Eip712Domain, K256PrivateKey, PackedEthSignature,
};

use crate::{
    raw_ethereum_tx::{Transaction, TransactionParameters},
    EthereumSigner, SignerError,
};

#[derive(Debug, Clone)]
pub struct PrivateKeySigner {
    private_key: K256PrivateKey,
}

// We define inherent methods duplicating `EthereumSigner` ones because they are sync and (other than `sign_typed_data`) infallible.
impl PrivateKeySigner {
    pub fn new(private_key: K256PrivateKey) -> Self {
        Self { private_key }
    }

    /// Gets an Ethereum address that matches this private key.
    pub fn address(&self) -> Address {
        self.private_key.address()
    }

    /// Signs typed struct using Ethereum private key by EIP-712 signature standard.
    /// Result of this function is the equivalent of RPC calling `eth_signTypedData`.
    pub fn sign_typed_data<S: EIP712TypedStructure + Sync>(
        &self,
        domain: &Eip712Domain,
        typed_struct: &S,
    ) -> Result<PackedEthSignature, SignerError> {
        let signature =
            PackedEthSignature::sign_typed_data(&self.private_key, domain, typed_struct)
                .map_err(|err| SignerError::SigningFailed(err.to_string()))?;
        Ok(signature)
    }

    /// Signs and returns the RLP-encoded transaction.
    pub fn sign_transaction(&self, raw_tx: TransactionParameters) -> Vec<u8> {
        // According to the code in web3 <https://docs.rs/web3/latest/src/web3/api/accounts.rs.html#86>
        // We should use `max_fee_per_gas` as `gas_price` if we use EIP1559
        let gas_price = raw_tx.max_fee_per_gas;
        let max_priority_fee_per_gas = raw_tx.max_priority_fee_per_gas;
        let tx = Transaction {
            to: raw_tx.to,
            nonce: raw_tx.nonce,
            gas: raw_tx.gas,
            gas_price,
            value: raw_tx.value,
            data: raw_tx.data,
            transaction_type: raw_tx.transaction_type,
            access_list: raw_tx.access_list.unwrap_or_default(),
            max_priority_fee_per_gas,
            max_fee_per_blob_gas: raw_tx.max_fee_per_blob_gas,
            blob_versioned_hashes: raw_tx.blob_versioned_hashes,
        };
        let signed = tx.sign(&self.private_key, raw_tx.chain_id);
        signed.raw_transaction.0
    }
}

#[async_trait]
impl EthereumSigner for PrivateKeySigner {
    async fn get_address(&self) -> Result<Address, SignerError> {
        Ok(self.address())
    }

    async fn sign_typed_data<S: EIP712TypedStructure + Sync>(
        &self,
        domain: &Eip712Domain,
        typed_struct: &S,
    ) -> Result<PackedEthSignature, SignerError> {
        self.sign_typed_data(domain, typed_struct)
    }

    async fn sign_transaction(
        &self,
        raw_tx: TransactionParameters,
    ) -> Result<Vec<u8>, SignerError> {
        Ok(self.sign_transaction(raw_tx))
    }
}

#[cfg(test)]
mod test {
    use zksync_basic_types::{H160, H256, U256, U64};
    use zksync_crypto_primitives::K256PrivateKey;

    use super::*;

    #[test]
    fn test_generating_signed_raw_transaction() {
        let private_key = K256PrivateKey::from_bytes(H256::from([5; 32])).unwrap();
        let signer = PrivateKeySigner::new(private_key);
        let raw_transaction = TransactionParameters {
            nonce: U256::from(1u32),
            to: Some(H160::default()),
            gas: Default::default(),
            gas_price: Some(U256::from(2u32)),
            max_fee_per_gas: U256::from(2u32),
            max_priority_fee_per_gas: U256::from(1u32),
            value: Default::default(),
            data: vec![1, 2, 3],
            chain_id: 270,
            transaction_type: Some(U64::from(1u32)),
            access_list: None,
            blob_versioned_hashes: None,
            max_fee_per_blob_gas: None,
        };
        let raw_tx = signer.sign_transaction(raw_transaction);
        assert_ne!(raw_tx.len(), 1);
        // pre-calculated signature with right algorithm implementation
        let precalculated_raw_tx: Vec<u8> = vec![
            1, 248, 100, 130, 1, 14, 1, 2, 128, 148, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 128, 131, 1, 2, 3, 192, 1, 160, 98, 201, 238, 158, 215, 98, 23, 231,
            221, 161, 170, 16, 54, 85, 187, 107, 12, 228, 218, 139, 103, 164, 17, 196, 178, 185,
            252, 243, 186, 175, 93, 230, 160, 93, 204, 205, 5, 46, 187, 231, 211, 102, 133, 200,
            254, 119, 94, 206, 81, 8, 143, 204, 14, 138, 43, 183, 214, 209, 166, 16, 116, 176, 44,
            52, 133,
        ];
        assert_eq!(raw_tx, precalculated_raw_tx);
    }
}
