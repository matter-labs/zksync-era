use serde::Deserialize;
use smart_config::{
    de::{FromSecretString, Optional, Serde},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::{secrets::PrivateKey, url::SensitiveUrl};

/// Configuration for the EigenDA remote disperser client.
///
/// This configuration is meant to be used by the EigenDA V2 client.
/// It is an insecure integration, where the dispersal is not verified.
#[derive(Clone, Debug, PartialEq, Deserialize, DescribeConfig, DeserializeConfig)]
pub struct EigenConfig {
    /// URL of the Disperser RPC server
    pub disperser_rpc: String,
    /// URL of the Ethereum RPC server
    #[config(secret, with = Optional(Serde![str]))]
    pub eigenda_eth_rpc: Option<SensitiveUrl>,
    /// Address of the EigenDA cert verifier router
    pub cert_verifier_router_addr: String,
    /// Blob version
    pub blob_version: u16,
    /// Address of the operator state retriever
    pub operator_state_retriever_addr: String,
    /// Address of the registry coordinator
    pub registry_coordinator_addr: String,
    /// URL of the EigenDA Prover Service RPC server
    /// This is used for EigenDA V2 Secure integration,
    /// so if its either `None` or `Some` defines whether we are using EigenDA V2 Secure or not.
    pub eigenda_prover_service_rpc: Option<String>,
}

/// Configuration for the EigenDA secrets.
#[derive(Clone, Debug, DescribeConfig, DeserializeConfig)]
pub struct EigenSecrets {
    /// Private key used for dispersing the blobs
    #[config(with = FromSecretString)]
    pub private_key: PrivateKey,
}
