//! Operator signer abstraction supporting local private keys and GCP KMS.
//!
//! This crate provides [`OperatorSigner`] which implements [`EthereumSigner`]
//! for use with [`SigningClient`](zksync_eth_signer).

use std::sync::Arc;

use alloy_primitives::B256;
use alloy_signer::Signer;
use alloy_signer_gcp::GcpSigner;
use async_trait::async_trait;
use tokio::sync::OnceCell;
use zksync_basic_types::{web3, Address, H256};
use zksync_crypto_primitives::{
    EIP712TypedStructure, Eip712Domain, K256PrivateKey, PackedEthSignature,
};
use zksync_eth_signer::{EthereumSigner, PrivateKeySigner, SignerError, TransactionParameters};

mod gcp;

/// Operator signer supporting both local private keys and GCP KMS.
///
/// For GCP KMS keys, the signer (and its underlying API client) is created lazily
/// on first use and cached for subsequent calls. Cloned instances share the same
/// cache via `Arc`, so only one GCP client is created regardless of how many
/// clones exist.
#[derive(Debug)]
pub enum OperatorSigner {
    /// Use a local private key for signing.
    Local(PrivateKeySigner),
    /// Use a Google Cloud KMS key for signing.
    GcpKms {
        /// Full resource name of the KMS key version, e.g.
        /// `projects/{project}/locations/{location}/keyRings/{ring}/cryptoKeys/{key}/cryptoKeyVersions/{version}`
        resource_name: String,
        /// Lazily-initialized GCP signer, shared across clones.
        cached_signer: Arc<OnceCell<GcpSigner>>,
    },
}

impl Clone for OperatorSigner {
    fn clone(&self) -> Self {
        match self {
            Self::Local(signer) => Self::Local(signer.clone()),
            Self::GcpKms {
                resource_name,
                cached_signer,
            } => Self::GcpKms {
                resource_name: resource_name.clone(),
                cached_signer: cached_signer.clone(),
            },
        }
    }
}

impl OperatorSigner {
    /// Creates a local-key signer.
    pub fn local(key: K256PrivateKey) -> Self {
        Self::Local(PrivateKeySigner::new(key))
    }

    /// Creates a GCP KMS signer config with an empty signer cache.
    pub fn gcp_kms(resource_name: String) -> Self {
        Self::GcpKms {
            resource_name,
            cached_signer: Arc::new(OnceCell::new()),
        }
    }

    /// Returns the cached GCP signer, creating it on first call.
    async fn get_gcp_signer(&self) -> Result<&GcpSigner, SignerError> {
        match self {
            Self::GcpKms {
                resource_name,
                cached_signer,
            } => cached_signer
                .get_or_try_init(|| gcp::create_gcp_signer(resource_name))
                .await
                .map_err(|e| SignerError::SigningFailed(e.to_string())),
            Self::Local(_) => unreachable!(),
        }
    }

    /// Returns the Ethereum address for this signer.
    ///
    /// For local keys the address is derived locally. For GCP KMS keys a network
    /// call is made on first invocation to fetch the public key; subsequent calls
    /// return the cached address.
    pub async fn address(&self) -> Result<Address, SignerError> {
        match self {
            Self::Local(signer) => Ok(signer.address()),
            Self::GcpKms { .. } => {
                let signer = self.get_gcp_signer().await?;
                Ok(Address::from_slice(signer.address().as_slice()))
            }
        }
    }

    /// Signs a hash via GCP KMS and converts the alloy signature to r/s/v.
    async fn gcp_sign_hash(&self, hash: &H256) -> Result<(H256, H256, u8), SignerError> {
        let signer = self.get_gcp_signer().await?;
        let sig = signer
            .sign_hash(&B256::from_slice(hash.as_bytes()))
            .await
            .map_err(|e| SignerError::SigningFailed(e.to_string()))?;
        let bytes = sig.as_bytes(); // [u8; 65] = r (32) || s (32) || v (1)
        Ok((
            H256::from_slice(&bytes[..32]),
            H256::from_slice(&bytes[32..64]),
            bytes[64],
        ))
    }
}

#[async_trait]
impl EthereumSigner for OperatorSigner {
    async fn get_address(&self) -> Result<Address, SignerError> {
        self.address().await
    }

    async fn sign_typed_data<S: EIP712TypedStructure + Sync>(
        &self,
        domain: &Eip712Domain,
        typed_struct: &S,
    ) -> Result<PackedEthSignature, SignerError> {
        match self {
            Self::Local(signer) => signer.sign_typed_data(domain, typed_struct),
            Self::GcpKms { .. } => {
                let hash = H256::from(
                    PackedEthSignature::typed_data_to_signed_bytes(domain, typed_struct).0,
                );
                let (r, s, v) = self.gcp_sign_hash(&hash).await?;
                Ok(PackedEthSignature::from_rsv(&r, &s, v))
            }
        }
    }

    async fn sign_transaction(
        &self,
        raw_tx: TransactionParameters,
    ) -> Result<Vec<u8>, SignerError> {
        match self {
            Self::Local(signer) => Ok(signer.sign_transaction(raw_tx)),
            Self::GcpKms { .. } => {
                let tx = zksync_eth_signer::Transaction {
                    to: raw_tx.to,
                    nonce: raw_tx.nonce,
                    gas: raw_tx.gas,
                    gas_price: raw_tx.max_fee_per_gas,
                    value: raw_tx.value,
                    data: raw_tx.data,
                    transaction_type: raw_tx.transaction_type,
                    access_list: raw_tx.access_list.unwrap_or_default(),
                    max_priority_fee_per_gas: raw_tx.max_priority_fee_per_gas,
                    max_fee_per_blob_gas: raw_tx.max_fee_per_blob_gas,
                    blob_versioned_hashes: raw_tx.blob_versioned_hashes,
                };

                let (message_hash, adjust_v_value) = tx.hash_for_signing(raw_tx.chain_id);
                let (r, s, v) = self.gcp_sign_hash(&message_hash).await?;

                let v = if adjust_v_value {
                    v as u64 + 35 + raw_tx.chain_id * 2
                } else {
                    v as u64
                };

                let signature = web3::Signature { r, s, v };
                Ok(tx.encode_with_signature(raw_tx.chain_id, &signature))
            }
        }
    }
}
