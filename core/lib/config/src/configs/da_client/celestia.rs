use serde::Deserialize;
use zksync_basic_types::secrets::PrivateKey;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct CelestiaConfig {
    // gRPC URL for celestia-app instance
    pub api_node_url: String,
    // gRPC URL of the Celestia eq-service instance
    // https://github.com/celestiaorg/eq-service
    pub eq_service_grpc_url: String,
    pub namespace: String,
    pub chain_id: String,
    pub timeout_ms: u64,
    // Tendermint RPC URL of the Celestia core instance
    pub celestia_core_tendermint_rpc_url: String,
    pub blobstream_contract_address: String,
    pub num_pages: u64,
    pub page_size: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CelestiaSecrets {
    pub private_key: PrivateKey,
}
