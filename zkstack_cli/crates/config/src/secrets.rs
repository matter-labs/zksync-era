use std::path::Path;

use common::{db::DatabaseConfig, yaml::PatchedConfig};
use xshell::Shell;
pub use zksync_config::configs::Secrets as SecretsConfig;

use crate::{
    consts::SECRETS_FILE,
    traits::{FileConfigWithDefaultName, ReadConfig},
};

pub fn set_server_database(
    secrets: &mut PatchedConfig,
    server_db_config: &DatabaseConfig,
) -> anyhow::Result<()> {
    secrets.insert(
        "database.server_url",
        server_db_config.full_url().to_string(),
    );
    Ok(())
}

pub fn set_prover_database(
    secrets: &mut PatchedConfig,
    prover_db_config: &DatabaseConfig,
) -> anyhow::Result<()> {
    secrets.insert(
        "database.prover_url",
        prover_db_config.full_url().to_string(),
    );
    Ok(())
}

pub fn set_l1_rpc_url(secrets: &mut PatchedConfig, l1_rpc_url: String) -> anyhow::Result<()> {
    secrets.insert("l1.l1_rpc_url", l1_rpc_url);
    Ok(())
}

impl FileConfigWithDefaultName for SecretsConfig {
    const FILE_NAME: &'static str = SECRETS_FILE;
}

impl ReadConfig for SecretsConfig {
    fn read(_shell: &Shell, _path: impl AsRef<Path>) -> anyhow::Result<Self> {
        todo!("remove")
    }
}
