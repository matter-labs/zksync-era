use smart_config::{
    de::{Delimited, FromSecretString, Serde},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::{secrets::PrivateKey, url::SensitiveUrl, Address};

#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(tag = "source")]
pub enum PointsSource {
    /// Load points from the provided path.
    Path {
        /// Path to a directory with the points files.
        path: String,
    },
    /// Load points from the provided URLs.
    Url {
        /// URL for G1 points.
        #[config(example = "https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g1.point".into())]
        g1_url: String,
        /// URL for G2 points.
        #[config(example = "https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g2.point.powerOf2".into())]
        g2_url: String,
    },
}

/// Configuration for the EigenDA remote disperser client.
#[derive(Clone, Debug, DescribeConfig, DeserializeConfig)]
pub struct EigenConfig {
    /// URL of the Disperser RPC server
    pub disperser_rpc: String,
    /// Block height needed to reach in order to consider the blob finalized
    /// a value less or equal to 0 means that the disperser will not wait for finalization
    #[config(default_t = 0)]
    pub settlement_layer_confirmation_depth: u32,
    /// URL of the Ethereum RPC server
    #[config(secret, with = Serde![str])]
    pub eigenda_eth_rpc: Option<SensitiveUrl>,
    /// Address of the service manager contract
    pub eigenda_svc_manager_address: Address,
    /// Wait for the blob to be finalized before returning the response
    #[config(default_t = true)]
    pub wait_for_finalization: bool,
    /// Whether to use authenticated dispersal.
    #[config(default_t = true)]
    pub authenticated: bool,
    /// Points source.
    #[config(nest)]
    pub points: PointsSource,
    /// Custom quorum numbers
    #[config(default, with = Delimited(","))]
    pub custom_quorum_numbers: Vec<u8>,
    #[config(with = FromSecretString, deprecated = ".secrets.private_key")]
    pub private_key: PrivateKey,
}
