use smart_config::{
    de::{FromSecretString, Optional, Serde},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::{secrets::PrivateKey, url::SensitiveUrl, Address};

#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(tag = "type")]
pub enum PointsSource {
    Path { path: String },
    Url { g1_url: String, g2_url: String },
}

/// Configuration for the EigenDA remote disperser client.
#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct EigenConfig {
    /// URL of the Disperser RPC server
    pub disperser_rpc: String,
    /// Block height needed to reach in order to consider the blob finalized
    /// a value less or equal to 0 means that the disperser will not wait for finalization
    #[config(default_t = 0)]
    pub settlement_layer_confirmation_depth: u32,
    /// URL of the Ethereum RPC server
    #[config(secret, with = Optional(Serde![str]))]
    pub eigenda_eth_rpc: Option<SensitiveUrl>,
    /// Address of the service manager contract
    pub eigenda_svc_manager_address: Address,
    /// Wait for the blob to be finalized before returning the response
    #[config(default_t = true)] // FIXME: double-check defaults
    pub wait_for_finalization: bool,
    /// Authenticated dispersal
    #[config(default_t = true)]
    pub authenticated: bool,
    /// Points source
    #[config(nest)]
    pub points_source: PointsSource,
    /// Custom quorum numbers
    #[config(default)]
    pub custom_quorum_numbers: Vec<u8>,
}

#[derive(Clone, Debug, DescribeConfig, DeserializeConfig)]
pub struct EigenSecrets {
    #[config(with = FromSecretString)]
    pub private_key: PrivateKey,
}
