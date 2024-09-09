pub const AMOUNT_FOR_DISTRIBUTION_TO_WALLETS: u128 = 1000000000000000000000;

pub const MINIMUM_BALANCE_FOR_WALLET: u128 = 5000000000000000000;
pub const SERVER_MIGRATIONS: &str = "core/lib/dal/migrations";
pub const PROVER_MIGRATIONS: &str = "prover/crates/lib/prover_dal/migrations";
pub const PROVER_STORE_MAX_RETRIES: u16 = 10;
pub const DEFAULT_CREDENTIALS_FILE: &str = "~/.config/gcloud/application_default_credentials.json";
pub const DEFAULT_PROOF_STORE_DIR: &str = "artifacts";
pub const BELLMAN_CUDA_DIR: &str = "era-bellman-cuda";
pub const L2_BASE_TOKEN_ADDRESS: &str = "0x000000000000000000000000000000000000800A";

pub const CONSENSUS_SECRETS_PATH: &str = "etc/env/consensus_secrets.yaml";

#[allow(non_upper_case_globals)]
const kB: usize = 1024;

/// Max payload size for consensus
pub const MAX_PAYLOAD_SIZE: usize = 2_500_000;
/// Max batch size for consensus
/// Compute a default batch size, so operators are not caught out by the missing setting
/// while we're still working on batch syncing. The batch interval is ~1 minute,
/// so there will be ~60 blocks, and an Ethereum Merkle proof is ~1kB, but under high
/// traffic there can be thousands of huge transactions that quickly fill up blocks
/// and there could be more blocks in a batch then expected. We chose a generous
/// limit so as not to prevent any legitimate batch from being transmitted.
pub const MAX_BATCH_SIZE: usize = MAX_PAYLOAD_SIZE * 5000 + kB;
/// Gossip dynamic inbound limit for consensus
pub const GOSSIP_DYNAMIC_INBOUND_LIMIT: usize = 1;

/// Path to the JS runtime config for the block-explorer-app docker container to be mounted to
pub const EXPLORER_APP_DOCKER_CONFIG_PATH: &str = "/usr/src/app/packages/app/dist/config.js";
pub const EXPLORER_APP_DOCKER_IMAGE: &str = "matterlabs/block-explorer-app";
/// Path to the JS runtime config for the dapp-portal docker container to be mounted to
pub const PORTAL_DOCKER_CONFIG_PATH: &str = "/usr/src/app/dist/config.js";
pub const PORTAL_DOCKER_IMAGE: &str = "matterlabs/dapp-portal";
