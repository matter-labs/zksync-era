pub const AMOUNT_FOR_DISTRIBUTION_TO_WALLETS: u128 = 1000000000000000000000;

pub const MINIMUM_BALANCE_FOR_WALLET: u128 = 5000000000000000000;
pub const SERVER_MIGRATIONS: &str = "core/lib/dal/migrations";
pub const PROVER_MIGRATIONS: &str = "prover/crates/lib/prover_dal/migrations";
pub const PROVER_STORE_MAX_RETRIES: u16 = 10;
pub const DEFAULT_CREDENTIALS_FILE: &str = "~/.config/gcloud/application_default_credentials.json";
pub const DEFAULT_PROOF_STORE_DIR: &str = "artifacts";
pub const BELLMAN_CUDA_DIR: &str = "era-bellman-cuda";
pub const L2_BASE_TOKEN_ADDRESS: &str = "0x000000000000000000000000000000000000800A";

/// Path to the JS runtime config for the block-explorer-app docker container to be mounted to
pub const EXPLORER_APP_DOCKER_CONFIG_PATH: &str = "/usr/src/app/packages/app/dist/config.js";
pub const EXPLORER_APP_DOCKER_IMAGE: &str = "matterlabs/block-explorer-app";
/// Path to the JS runtime config for the dapp-portal docker container to be mounted to
pub const PORTAL_DOCKER_CONFIG_PATH: &str = "/usr/src/app/dist/config.js";
pub const PORTAL_DOCKER_IMAGE: &str = "matterlabs/dapp-portal";

pub const PROVER_GATEWAY_DOCKER_IMAGE: &str = "matterlabs/prover-fri-gateway:latest2.0";
pub const WITNESS_GENERATOR_DOCKER_IMAGE: &str = "matterlabs/witness-generator:latest2.0";
pub const WITNESS_VECTOR_GENERATOR_DOCKER_IMAGE: &str =
    "matterlabs/witness-vector-generator:latest2.0";
pub const PROVER_DOCKER_IMAGE: &str = "matterlabs/prover-gpu-fri:latest2.0";
pub const COMPRESSOR_DOCKER_IMAGE: &str = "matterlabs/proof-fri-gpu-compressor:latest2.0";
pub const PROVER_JOB_MONITOR_DOCKER_IMAGE: &str = "matterlabs/prover-job-monitor:latest2.0";

pub const PROVER_GATEWAY_BINARY_NAME: &str = "zksync_prover_fri_gateway";
pub const WITNESS_GENERATOR_BINARY_NAME: &str = "zksync_witness_generator";
pub const WITNESS_VECTOR_GENERATOR_BINARY_NAME: &str = "zksync_witness_vector_generator";
pub const PROVER_BINARY_NAME: &str = "zksync_prover_fri";
pub const COMPRESSOR_BINARY_NAME: &str = "zksync_proof_fri_compressor";
pub const PROVER_JOB_MONITOR_BINARY_NAME: &str = "zksync_prover_job_monitor";
