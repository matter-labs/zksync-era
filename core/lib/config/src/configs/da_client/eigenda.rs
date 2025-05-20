use serde::Deserialize;
use serde::Serialize;
use zksync_basic_types::{secrets::PrivateKey, url::SensitiveUrl, Address};
use smart_config::{
    de::{Delimited, FromSecretString, Optional, Serde, WellKnown},
    DescribeConfig, DeserializeConfig,
};

/// Describes the different ways a polynomial may be represented
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default, Serialize)]
pub enum PolynomialForm {
    /// Coeff is short for polynomial "coefficient form".
    /// The field elements represent the coefficients of the polynomial.
    #[default]
    #[serde(rename = "coeff")]
    Coeff,
    /// Eval is short for polynomial "evaluation form".
    /// The field elements represent the evaluation of the polynomial at roots of unity.
    #[serde(rename = "eval")]
    Eval,
}

impl PolynomialForm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Coeff => "coeff",
            Self::Eval => "eval",
        }
    }
}

impl WellKnown for PolynomialForm {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

/// The source of the points used for dispersing the blobs.
/// It can be either a path to a local file or a URL.
#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig, Deserialize)]
#[config(tag = "source")]
pub enum PointsSource {
    /// Path to a local file
    Path { path: String },
    /// g1_url, g2_url
    Url { g1_url: String, g2_url: String },
}

/// The EigenDA client has two versions: V1 and V2.
/// The V1 client is the original EigenDA client, while the V2 client is a new version.
/// This enum is used to differentiate between the two versions.
#[derive(Clone, Debug, PartialEq, Deserialize, DescribeConfig, DeserializeConfig)]
#[config(tag = "version")]
pub enum VersionSpecificConfig {
    /// The EigenDA V1 client
    V1(V1Config),
    /// The EigenDA V2 client
    V2(V2Config),
}

/// Configuration fields unique of the EigenDA V1 client.
#[derive(Clone, Debug, PartialEq, Deserialize, DescribeConfig, DeserializeConfig)]
pub struct V1Config {
    /// Block height needed to reach in order to consider the blob finalized
    /// a value less or equal to 0 means that the disperser will not wait for finalization
    #[config(default_t = 0)]
    pub settlement_layer_confirmation_depth: u32,
    /// Address of the service manager contract
    pub eigenda_svc_manager_address: Address,
    /// Wait for the blob to be finalized before returning the response
    #[config(default_t = true)]
    pub wait_for_finalization: bool,
    /// Points source
    #[config(nest)]
    pub points_source: PointsSource,
    /// Custom quorum numbers
    pub custom_quorum_numbers: Vec<u8>,
}

/// Configuration fields unique of the EigenDA V2 client.
#[derive(Clone, Debug, PartialEq, Deserialize, DescribeConfig, DeserializeConfig)]
pub struct V2Config {
    /// Address of the EigenDA cert verifier
    pub cert_verifier_addr: Address,
    /// Blob version
    pub blob_version: u16,
    /// Polynomial form to disperse the blobs
    #[serde(default)]
    pub polynomial_form: PolynomialForm,
}

/// Configuration for the EigenDA remote disperser client.
///
/// This configuration is meant to be used by both the EigenDA V1 and V2 client.
/// The M0 stands for Milestone 0, an insecure integration, where the dispersal is not verified.
#[derive(Clone, Debug, PartialEq, Deserialize, DescribeConfig, DeserializeConfig)]
pub struct EigenDAConfig {
    // Shared fields between V1 and V2
    /// URL of the Disperser RPC server
    pub disperser_rpc: String,
    /// URL of the Ethereum RPC server
    #[config(secret, with = Optional(Serde![str]))]
    pub eigenda_eth_rpc: Option<SensitiveUrl>,
    /// Authenticated dispersal
    #[config(default_t = true)]
    pub authenticated: bool,
    /// Config specific to each version
    #[config(nest)]
    pub version_specific: VersionSpecificConfig,
}

/// Configuration for the EigenDA secrets.
#[derive(Clone, Debug, DescribeConfig, DeserializeConfig)]
pub struct EigenDASecrets {
    /// Private key used for dispersing the blobs
    #[config(with = FromSecretString)]
    pub private_key: PrivateKey,
}
