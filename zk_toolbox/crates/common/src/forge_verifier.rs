use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use strum_macros::Display;

#[derive(Parser, Debug, Clone, Serialize, Deserialize)]
pub struct VerifierArgs {
    /// Verify deployed contracts
    #[clap(long, global = true, default_missing_value = "true", num_args = 0..=1)]
    pub verify: Option<bool>,
    /// Verifier to use
    #[clap(long, global = true, default_value_t = Verifier::Etherscan)]
    pub verifier: Verifier,
    /// Verifier URL, if using a custom provider
    #[clap(long, global = true)]
    pub verifier_url: Option<String>,
    /// Verifier API key
    #[clap(long, global = true)]
    pub verifier_api_key: Option<String>,
}

#[derive(Debug, Clone, ValueEnum, Display, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum Verifier {
    Etherscan,
    Sourcify,
    Blockscout,
    Oklink,
}
