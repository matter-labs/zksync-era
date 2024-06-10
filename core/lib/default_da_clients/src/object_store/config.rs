use zksync_config::ObjectStoreConfig;
use zksync_env_config::envy_load;

pub struct ObjectStoreDAConfig {
    pub config: ObjectStoreConfig,
}

impl ObjectStoreDAConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            config: envy_load("object_store", "OBJECT_STORE_DA_CLIENT_")?,
        })
    }
}
