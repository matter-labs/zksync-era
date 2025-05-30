#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

pub use crate::configs::{
    contracts::chain::ContractsConfig, full_config_schema, ApiConfig, AvailConfig,
    BaseTokenAdjusterConfig, CelestiaConfig, ContractVerifierConfig, DAClientConfig,
    DADispatcherConfig, DBConfig, EigenDAConfig, EthConfig, EthWatchConfig,
    ExternalProofIntegrationApiConfig, GasAdjusterConfig, GenesisConfig, ObjectStoreConfig,
    PostgresConfig, SnapshotsCreatorConfig,
};
#[cfg(feature = "observability_ext")]
pub use crate::observability_ext::ConfigRepositoryExt;

#[cfg(feature = "cli")]
pub mod cli;
pub mod configs;
#[cfg(feature = "observability_ext")]
mod observability_ext;
pub mod sources;
#[cfg(test)]
mod tests;
mod utils;
