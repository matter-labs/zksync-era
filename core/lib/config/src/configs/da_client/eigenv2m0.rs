use serde::Deserialize;
use zksync_basic_types::{secrets::PrivateKey, url::SensitiveUrl, Address};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum PolynomialForm {
    Coeff,
    Eval,
}

/// Configuration for the EigenDA remote disperser client.
#[derive(Clone, Debug, PartialEq, Deserialize)]
<<<<<<<< HEAD:core/lib/config/src/configs/da_client/eigenv2m1.rs
pub struct EigenConfigV2M1 {
|||||||| 1c8c3a573:core/lib/config/src/configs/da_client/eigen.rs
pub struct EigenConfig {
========
pub struct EigenConfigV2M0 {
>>>>>>>> eigenda-v2-m0:core/lib/config/src/configs/da_client/eigenv2m0.rs
    /// URL of the Disperser RPC server
    pub disperser_rpc: String,
    /// URL of the Ethereum RPC server
    pub eigenda_eth_rpc: Option<SensitiveUrl>,
    /// Authenticated dispersal
    pub authenticated: bool,
    /// Address of the eigenDA registry contract
    pub eigenda_cert_and_blob_verifier_addr: Address,
    /// Address of the EigenDA cert verifier
    pub cert_verifier_addr: Address,
    /// Blob version
    pub blob_version: u16,
    /// Polynomial form to disperse the blobs
    pub polynomial_form: PolynomialForm,
}

#[derive(Clone, Debug, PartialEq)]
<<<<<<<< HEAD:core/lib/config/src/configs/da_client/eigenv2m1.rs
pub struct EigenSecretsV2M1 {
|||||||| 1c8c3a573:core/lib/config/src/configs/da_client/eigen.rs
pub struct EigenSecrets {
========
pub struct EigenSecretsV2M0 {
>>>>>>>> eigenda-v2-m0:core/lib/config/src/configs/da_client/eigenv2m0.rs
    pub private_key: PrivateKey,
}
