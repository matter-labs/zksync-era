pub const AMOUNT_FOR_DISTRIBUTION_TO_WALLETS: u128 = 1000000000000000000000;

pub const MINIMUM_BALANCE_FOR_WALLET: u128 = 5000000000000000000;
/// The default block range within which we search for events within one query.
pub const DEFAULT_EVENTS_BLOCK_RANGE: u64 = 50_000;
pub const SERVER_MIGRATIONS: &str = "core/lib/dal/migrations";
pub const PROVER_MIGRATIONS: &str = "prover/crates/lib/prover_dal/migrations";
pub const PROVER_STORE_MAX_RETRIES: u16 = 10;
pub const DEFAULT_CREDENTIALS_FILE: &str = "~/.config/gcloud/application_default_credentials.json";
pub const DEFAULT_PROOF_STORE_DIR: &str = "artifacts";
pub const DEFAULT_UNSIGNED_TRANSACTIONS_DIR: &str = "transactions";
pub const BELLMAN_CUDA_DIR: &str = "era-bellman-cuda";
pub const L2_BASE_TOKEN_ADDRESS: &str = "0x000000000000000000000000000000000000800A";

/// Path to the JS runtime config for the block-explorer-app docker container to be mounted to
pub const EXPLORER_APP_DOCKER_CONFIG_PATH: &str = "/usr/src/app/packages/app/dist/config.js";
pub const EXPLORER_APP_DOCKER_IMAGE: &str = "matterlabs/block-explorer-app:v2.73.1";
/// Path to the JS runtime config for the dapp-portal docker container to be mounted to
pub const PORTAL_DOCKER_CONFIG_PATH: &str = "/usr/src/app/dist/config.js";
pub const PORTAL_DOCKER_IMAGE: &str = "matterlabs/dapp-portal";

pub const PROVER_GATEWAY_DOCKER_IMAGE: &str = "matterlabs/prover-fri-gateway";
pub const WITNESS_GENERATOR_DOCKER_IMAGE: &str = "matterlabs/witness-generator";
pub const CIRCUIT_PROVER_DOCKER_IMAGE: &str = "matterlabs/circuit-prover-gpu";
pub const COMPRESSOR_DOCKER_IMAGE: &str = "matterlabs/proof-fri-gpu-compressor";
pub const PROVER_JOB_MONITOR_DOCKER_IMAGE: &str = "matterlabs/prover-job-monitor";

pub const PROVER_GATEWAY_BINARY_NAME: &str = "zksync_prover_fri_gateway";
pub const WITNESS_GENERATOR_BINARY_NAME: &str = "zksync_witness_generator";
pub const CIRCUIT_PROVER_BINARY_NAME: &str = "zksync_circuit_prover";
pub const COMPRESSOR_BINARY_NAME: &str = "zksync_proof_fri_compressor";
pub const PROVER_JOB_MONITOR_BINARY_NAME: &str = "zksync_prover_job_monitor";

pub const PATH_TO_ONLY_REAL_PROOFS_OVERRIDE_CONFIG: &str =
    "etc/env/file_based/overrides/only_real_proofs.yaml";
pub const PATH_TO_VALIDIUM_OVERRIDE_CONFIG: &str = "etc/env/file_based/overrides/validium.yaml";

pub const PATH_TO_GATEWAY_OVERRIDE_CONFIG: &str = "etc/env/file_based/overrides/gateway.yaml";
