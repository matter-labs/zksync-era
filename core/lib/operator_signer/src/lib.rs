//! Operator signer abstraction supporting local private keys and GCP KMS.
//!
//! This crate provides [`SignerConfig`] for configuration and [`OperatorSigner`] which
//! implements [`EthereumSigner`] for use with [`SigningClient`](zksync_eth_signer).

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

/// Configuration for how a signing key is provided.
///
/// This is a pure configuration type — it describes *where* the key lives but does
/// not hold any initialized signer. Use [`OperatorSigner::from_config`] to create
/// an active signer from this configuration.
#[derive(Debug, Clone)]
pub enum SignerConfig {
    /// Use a local private key for signing.
    Local(K256PrivateKey),
    /// Use a Google Cloud KMS key for signing.
    GcpKms {
        /// Full resource name of the KMS key version, e.g.
        /// `projects/{project}/locations/{location}/keyRings/{ring}/cryptoKeys/{key}/cryptoKeyVersions/{version}`
        resource_name: String,
    },
}

/// An operator signer that implements [`EthereumSigner`].
///
/// Supports both local private key signing and remote GCP KMS signing.
/// For GCP KMS, the signer is lazily initialized and cached.
#[derive(Clone, Debug)]
enum OperatorSignerInner {
    Local(PrivateKeySigner),
    GcpKms {
        resource_name: String,
        cached_signer: Arc<OnceCell<GcpSigner>>,
    },
}

#[derive(Clone, Debug)]
pub struct OperatorSigner {
    inner: OperatorSignerInner,
}

impl OperatorSigner {
    /// Creates an [`OperatorSigner`] from a [`SignerConfig`].
    ///
    /// For local keys, the signer is created immediately.
    /// For GCP KMS, the signer is lazily initialized on first use.
    pub fn from_config(config: SignerConfig) -> Self {
        match config {
            SignerConfig::Local(key) => Self {
                inner: OperatorSignerInner::Local(PrivateKeySigner::new(key)),
            },
            SignerConfig::GcpKms { resource_name } => Self {
                inner: OperatorSignerInner::GcpKms {
                    resource_name,
                    cached_signer: Arc::new(OnceCell::new()),
                },
            },
        }
    }

    /// Creates an [`OperatorSigner`] from a [`K256PrivateKey`].
    pub fn from_private_key(key: K256PrivateKey) -> Self {
        Self {
            inner: OperatorSignerInner::Local(PrivateKeySigner::new(key)),
        }
    }

    /// Returns the Ethereum address for this signer.
    ///
    /// For local keys the address is derived locally. For GCP KMS keys a network
    /// call is made on first invocation to fetch the public key; subsequent calls
    /// return the cached address.
    pub async fn address(&self) -> Result<Address, SignerError> {
        match &self.inner {
            OperatorSignerInner::Local(signer) => Ok(signer.address()),
            OperatorSignerInner::GcpKms { .. } => {
                let gcp = self.get_gcp_signer().await?;
                Ok(alloy_address_to_zksync(gcp.address()))
            }
        }
    }

    async fn get_gcp_signer(&self) -> Result<&GcpSigner, SignerError> {
        match &self.inner {
            OperatorSignerInner::GcpKms {
                resource_name,
                cached_signer,
            } => cached_signer
                .get_or_try_init(|| gcp::create_signer(resource_name))
                .await
                .map_err(|e| SignerError::SigningFailed(e.to_string())),
            OperatorSignerInner::Local(_) => unreachable!(),
        }
    }

    /// Signs a hash via GCP KMS and converts the alloy signature to zksync types.
    async fn gcp_sign_hash(&self, hash: &H256) -> Result<(H256, H256, u8), SignerError> {
        let gcp = self.get_gcp_signer().await?;
        let sig = gcp
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
        match &self.inner {
            OperatorSignerInner::Local(signer) => signer.sign_typed_data(domain, typed_struct),
            OperatorSignerInner::GcpKms { .. } => {
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
        match &self.inner {
            OperatorSignerInner::Local(signer) => Ok(signer.sign_transaction(raw_tx)),
            OperatorSignerInner::GcpKms { .. } => {
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

fn alloy_address_to_zksync(addr: alloy_primitives::Address) -> Address {
    Address::from_slice(addr.as_slice())
}
