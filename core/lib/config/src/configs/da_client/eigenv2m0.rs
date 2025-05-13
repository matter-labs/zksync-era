use serde::Deserialize;
use zksync_basic_types::{secrets::PrivateKey, url::SensitiveUrl, Address};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum PolynomialForm {
    Coeff,
    Eval,
}

/// Configuration for the EigenDA remote disperser client.
///
/// This configuration is meant to be used by the EigenDA V2 client.
/// The M0 stands for Milestone 0, an insecure integration, where the dispersal is not verified.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct EigenConfigV2M0 {
    /// URL of the Disperser RPC server
    pub disperser_rpc: String,
    /// URL of the Ethereum RPC server
    pub eigenda_eth_rpc: Option<SensitiveUrl>,
    /// Authenticated dispersal
    pub authenticated: bool,
    /// Address of the EigenDA cert verifier
    pub cert_verifier_addr: Address,
    /// Blob version
    pub blob_version: u16,
    /// Polynomial form to disperse the blobs
    pub polynomial_form: PolynomialForm,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EigenSecretsV2M0 {
    pub private_key: PrivateKey,
}
