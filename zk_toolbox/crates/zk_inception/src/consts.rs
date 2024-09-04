pub const AMOUNT_FOR_DISTRIBUTION_TO_WALLETS: u128 = 1000000000000000000000;

pub const MINIMUM_BALANCE_FOR_WALLET: u128 = 5000000000000000000;
pub const SERVER_MIGRATIONS: &str = "core/lib/dal/migrations";
pub const PROVER_MIGRATIONS: &str = "prover/crates/lib/prover_dal/migrations";
pub const PROVER_STORE_MAX_RETRIES: u16 = 10;
pub const DEFAULT_CREDENTIALS_FILE: &str = "~/.config/gcloud/application_default_credentials.json";
pub const DEFAULT_PROOF_STORE_DIR: &str = "artifacts";
pub const BELLMAN_CUDA_DIR: &str = "era-bellman-cuda";
pub const L2_BASE_TOKEN_ADDRESS: &str = "0x000000000000000000000000000000000000800A";
pub const PORTAL_DOCKER_IMAGE: &str = "matterlabs/dapp-portal";
pub const PORTAL_DOCKER_CONTAINER_PORT: u16 = 3000;
