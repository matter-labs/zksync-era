use std::{cell::OnceCell, path::PathBuf};

use xshell::Shell;
use zksync_config::configs::{
    fri_prover_group::FriProverGroupConfig, FriProofCompressorConfig, FriProverConfig,
    FriProverGatewayConfig, FriWitnessGeneratorConfig, FriWitnessVectorGeneratorConfig,
};

use crate::traits::ZkToolboxConfig;

pub struct GeneralProverConfig {
    pub name: String,
    pub link_to_code: PathBuf,
    pub bellman_cuda_dir: Option<PathBuf>,
    pub config: PathBuf,
    pub shell: OnceCell<Shell>,
}

pub struct ProverConfig {
    pub fri_prover_config: FriProverConfig,
    pub fri_witness_generator_config: FriWitnessGeneratorConfig,
    pub fri_witness_vector_generator_config: FriWitnessVectorGeneratorConfig,
    pub fri_prover_gateway_config: FriProverGatewayConfig,
    pub fri_proof_compressor_config: FriProofCompressorConfig,
    pub fri_prover_group_config: FriProverGroupConfig,
}

impl ZkToolboxConfig for GeneralProverConfig {}
impl ZkToolboxConfig for ProverConfig {}
