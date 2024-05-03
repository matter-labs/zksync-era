use zksync_config::SnapshotsCreatorConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for SnapshotsCreatorConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("snapshots_creator", "SNAPSHOTS_CREATOR_")
    }
}
