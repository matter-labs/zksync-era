use std::time::Duration;

use smart_config::{de::FromSecretString, DescribeConfig, DeserializeConfig};
use zksync_basic_types::secrets::PrivateKey;

#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct CelestiaConfig {
    // gRPC URL for celestia-app instance
    pub api_node_url: String,
    pub namespace: String,
    pub chain_id: String,
    #[config(default_t = Duration::from_secs(30))]
    pub timeout: Duration,
    #[config(nest)]
    pub inclusion_verification: Option<CelestiaInclusionVerificationConfig>,
}

#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct CelestiaInclusionVerificationConfig {
    // gRPC URL of the Celestia eq-service instance
    // https://github.com/celestiaorg/eq-service
    pub eq_service_grpc_url: String,
    // Tendermint RPC URL of the Celestia core instance
    pub celestia_core_tendermint_rpc_url: String,
    pub blobstream_contract_address: String,
    #[config(default_t = 500)]
    pub blobstream_events_num_pages: u64,
    #[config(default_t = 1000)]
    pub blobstream_events_page_size: u64,
}

#[derive(Clone, Debug, DescribeConfig, DeserializeConfig)]
pub struct CelestiaSecrets {
    #[config(with = FromSecretString)]
    pub private_key: PrivateKey,
}
