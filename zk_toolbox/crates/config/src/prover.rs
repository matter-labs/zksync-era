use std::{cell::OnceCell, path::PathBuf};

use thiserror::Error;
use xshell::Shell;
use zksync_config::{
    configs::{
        fri_prover_group::FriProverGroupConfig, FriProofCompressorConfig, FriProverConfig,
        FriProverGatewayConfig, FriWitnessGeneratorConfig, FriWitnessVectorGeneratorConfig,
    },
    PostgresConfig,
};

use crate::{
    consts::{CONFIG_NAME, PROVER_CONFIG_NAME},
    find_file,
    traits::{FileConfigWithDefaultName, ZkToolboxConfig},
    ChainConfigInternal, EcosystemConfig, EcosystemConfigFromFileError, PROVER_FILE,
};

pub struct GeneralProverConfig {
    pub name: String,
    pub link_to_code: PathBuf,
    pub bellman_cuda_dir: Option<PathBuf>,
    pub config: PathBuf,
    pub shell: OnceCell<Shell>,
}

pub struct ProverConfig {
    pub postgres_config: PostgresConfig,
    pub fri_prover_config: FriProverConfig,
    pub fri_witness_generator_config: FriWitnessGeneratorConfig,
    pub fri_witness_vector_generator_config: FriWitnessVectorGeneratorConfig,
    pub fri_prover_gateway_config: FriProverGatewayConfig,
    pub fri_proof_compressor_config: FriProofCompressorConfig,
    pub fri_prover_group_config: FriProverGroupConfig,
}

impl FileConfigWithDefaultName for ProverConfig {
    const FILE_NAME: &'static str = PROVER_FILE;
}

impl ZkToolboxConfig for GeneralProverConfig {}
impl ZkToolboxConfig for ProverConfig {}

/// Result of checking if the ecosystem exists.
#[derive(Error, Debug)]
pub enum GeneralProverConfigFromFileError {
    #[error("General prover configuration not found (Could not find 'ProverSubsystem.toml' in {path:?}: Make sure you have created a subsystem & are in the new folder `cd path/to/prover/subsystem/name`)"
    )]
    NotExists { path: PathBuf },
    #[error("Invalid subsystem configuration")]
    InvalidConfig { source: anyhow::Error },
}

impl GeneralProverConfig {
    fn get_shell(&self) -> &Shell {
        self.shell.get().expect("Must be initialized")
    }

    pub fn from_file(shell: &Shell) -> Result<Self, GeneralProverConfigFromFileError> {
        let Ok(path) = find_file(shell, shell.current_dir(), PROVER_CONFIG_NAME) else {
            return Err(GeneralProverConfigFromFileError::NotExists {
                path: shell.current_dir(),
            });
        };

        shell.change_dir(&path);

        let prover = match GeneralProverConfig::read(shell, CONFIG_NAME) {
            Ok(mut config) => {
                config.shell = shell.clone().into();
                config
            }
            Err(_) => {
                let current_dir = shell.current_dir();
                let Some(parent) = current_dir.parent() else {
                    return Err(GeneralProverConfigFromFileError::NotExists { path });
                };
                // Try to find subsystem somewhere in parent directories
                shell.change_dir(parent);
                let mut prover_config = GeneralProverConfig::from_file(shell)?;
                prover_config
            }
        };
        Ok(prover)
    }
}
