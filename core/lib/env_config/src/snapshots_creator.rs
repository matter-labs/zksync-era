use crate::{envy_load, FromEnv};
use zksync_config::SnapshotsCreatorConfig;

impl FromEnv for SnapshotsCreatorConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("snapshots_creator", "SNAPSHOTS_CREATOR_")
    }
}
