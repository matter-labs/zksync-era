use zksync_config::ObjectStoreConfig;
use zksync_env_config::envy_load;

#[derive(Debug)]
pub struct DAObjectStoreConfig(pub ObjectStoreConfig);

impl DAObjectStoreConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let config = envy_load("object_store", "DA_CLIENT_OBJECT_STORE_")?;
        Ok(Self(config))
    }
}
