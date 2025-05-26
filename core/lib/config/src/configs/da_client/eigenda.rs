use serde::{Deserialize, Serialize};
use smart_config::{
    de::{Delimited, FromSecretString, Optional, Serde, WellKnown},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::{secrets::PrivateKey, url::SensitiveUrl, Address};

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
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum Version {
    /// The EigenDA V1 client
    V1,
    /// The EigenDA V2 client
    V2,
    /// The EigenDA V2 client with secure integration
    V2Secure,
}

impl Version {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "V1",
            Self::V2 => "V2",
            Self::V2Secure => "V2Secure",
        }
    }
}

impl WellKnown for Version {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

/// Configuration for the EigenDA remote disperser client.
///
/// This configuration is meant to be used by both the EigenDA V1 and V2 client.
/// The M0 stands for Milestone 0, an insecure integration, where the dispersal is not verified.
#[derive(Clone, Debug, PartialEq, Deserialize, DescribeConfig, DeserializeConfig)]
pub struct EigenDAConfig {
    // Shared fields between V1 and V2
    //
    /// URL of the Disperser RPC server
    pub disperser_rpc: String,
    /// URL of the Ethereum RPC server
    #[config(secret, with = Optional(Serde![str]))]
    pub eigenda_eth_rpc: Option<SensitiveUrl>,
    /// Authenticated dispersal
    #[config(default_t = true)]
    pub authenticated: bool,
    /// Config specific to each version
    pub version: Version,
    // V1 specific fields
    //
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
    pub points: PointsSource,
    /// Custom quorum numbers
    #[config(default, with = Delimited(","))]
    pub custom_quorum_numbers: Vec<u8>,
    // V2 and M1 specific fields
    //
    /// Address of the EigenDA cert verifier
    pub cert_verifier_addr: Address,
    /// Blob version
    pub blob_version: u16,
    /// Polynomial form to disperse the blobs
    #[serde(default)]
    pub polynomial_form: PolynomialForm,
    // V2Secure specific fields
    //
    /// URL of the EigenDA Sidecar RPC server
    pub eigenda_sidecar_rpc: String,
}

/// Configuration for the EigenDA secrets.
#[derive(Clone, Debug, DescribeConfig, DeserializeConfig)]
pub struct EigenDASecrets {
    /// Private key used for dispersing the blobs
    #[config(with = FromSecretString)]
    pub private_key: PrivateKey,
}
