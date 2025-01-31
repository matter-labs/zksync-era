use zksync_config::SnapshotsCreatorConfig;

use crate::{envy_load, object_store::SnapshotsObjectStoreConfig, FromEnv};

impl FromEnv for SnapshotsCreatorConfig {
    fn from_env() -> anyhow::Result<Self> {
        let mut snapshot_creator: SnapshotsCreatorConfig =
            envy_load("snapshots_creator", "SNAPSHOTS_CREATOR_")?;

        snapshot_creator.object_store = SnapshotsObjectStoreConfig::from_env().map(|a| a.0).ok();
        Ok(snapshot_creator)
    }
}
