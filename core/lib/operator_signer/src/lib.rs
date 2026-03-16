//! Operator signer abstraction supporting local private keys and GCP KMS.
//!
//! This crate provides [`SignerConfig`] for configuration and [`OperatorSigner`] which
//! implements [`EthereumSigner`] for use with [`SigningClient`](zksync_eth_signer).

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::OnceCell;
use zksync_basic_types::{Address, H256};
use zksync_crypto_primitives::{
    EIP712TypedStructure, Eip712Domain, K256PrivateKey, PackedEthSignature,
};
use zksync_eth_signer::{EthereumSigner, PrivateKeySigner, SignerError, TransactionParameters};

mod gcp;
use gcp::GcpKmsSigner;

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
#[allow(private_interfaces)]
pub enum OperatorSigner {
    /// Local private key signer.
    Local(PrivateKeySigner),
    /// GCP KMS signer (lazily initialized, shared across clones via Arc).
    GcpKms {
        resource_name: String,
        cached_signer: Arc<OnceCell<GcpKmsSigner>>,
    },
}

impl OperatorSigner {
    /// Creates an [`OperatorSigner`] from a [`SignerConfig`].
    ///
    /// For local keys, the signer is created immediately.
    /// For GCP KMS, the signer is lazily initialized on first use.
    pub fn from_config(config: SignerConfig) -> Self {
        match config {
            SignerConfig::Local(key) => Self::Local(PrivateKeySigner::new(key)),
            SignerConfig::GcpKms { resource_name } => Self::GcpKms {
                resource_name,
                cached_signer: Arc::new(OnceCell::new()),
            },
        }
    }

    /// Creates an [`OperatorSigner`] from a [`K256PrivateKey`].
    pub fn from_private_key(key: K256PrivateKey) -> Self {
        Self::Local(PrivateKeySigner::new(key))
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
                let gcp = self.get_gcp_signer().await?;
                Ok(gcp.address())
            }
        }
    }

    async fn get_gcp_signer(&self) -> Result<&GcpKmsSigner, SignerError> {
        match self {
            Self::GcpKms {
                resource_name,
                cached_signer,
            } => cached_signer
                .get_or_try_init(|| async {
                    GcpKmsSigner::new(resource_name.clone()).await
                })
                .await
                .map_err(|e| SignerError::SigningFailed(e.to_string())),
            Self::Local(_) => Err(SignerError::SigningFailed(
                "get_gcp_signer called on Local variant".to_string(),
            )),
        }
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
                let gcp = self.get_gcp_signer().await?;
                let signed_bytes =
                    H256::from(PackedEthSignature::typed_data_to_signed_bytes(domain, typed_struct).0);
                gcp.sign_hash_raw(&signed_bytes)
                    .await
                    .map_err(|e| SignerError::SigningFailed(e.to_string()))
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
                let gcp = self.get_gcp_signer().await?;

                let gas_price = raw_tx.max_fee_per_gas;
                let max_priority_fee_per_gas = raw_tx.max_priority_fee_per_gas;

                let tx = zksync_eth_signer::Transaction {
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

                let (message_hash, adjust_v_value) = tx.hash_for_signing(raw_tx.chain_id);

                let chain_id_for_v = if adjust_v_value {
                    Some(raw_tx.chain_id)
                } else {
                    None
                };
                let signature = gcp
                    .sign_hash(&message_hash, chain_id_for_v)
                    .await
                    .map_err(|e| SignerError::SigningFailed(e.to_string()))?;

                let signed_tx = tx.encode_with_signature(raw_tx.chain_id, &signature);
                Ok(signed_tx)
            }
        }
    }
}
