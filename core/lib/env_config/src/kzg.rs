use zksync_config::configs::KzgConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for KzgConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("kzg", "KZG_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> KzgConfig {
        KzgConfig {
            trusted_setup_path: "dir/file.json".to_owned(),
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            KZG_TRUSTED_SETUP_PATH="dir/file.json"
        "#;
        lock.set_env(config);

        let actual = KzgConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
