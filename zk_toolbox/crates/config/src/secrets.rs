use std::{path::Path, str::FromStr};

use anyhow::Context;
use common::db::DatabaseConfig;
use xshell::Shell;
use zksync_basic_types::url::SensitiveUrl;
pub use zksync_config::configs::Secrets as SecretsConfig;
use zksync_protobuf_config::{decode_yaml_repr, encode_yaml_repr};

use crate::{
    consts::SECRETS_FILE,
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfig},
};

pub fn set_databases(
    secrets: &mut SecretsConfig,
    server_db_config: &DatabaseConfig,
    prover_db_config: &DatabaseConfig,
) -> anyhow::Result<()> {
    let database = secrets
        .database
        .as_mut()
        .context("Databases must be presented")?;
    database.server_url = Some(SensitiveUrl::from(server_db_config.full_url()));
    database.prover_url = Some(SensitiveUrl::from(prover_db_config.full_url()));
    Ok(())
}

pub fn set_l1_rpc_url(secrets: &mut SecretsConfig, l1_rpc_url: String) -> anyhow::Result<()> {
    secrets
        .l1
        .as_mut()
        .context("L1 Secrets must be presented")?
        .l1_rpc_url = SensitiveUrl::from_str(&l1_rpc_url)?;
    Ok(())
}

impl FileConfigWithDefaultName for SecretsConfig {
    const FILE_NAME: &'static str = SECRETS_FILE;
}

impl SaveConfig for SecretsConfig {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let bytes = encode_yaml_repr::<zksync_protobuf_config::proto::secrets::Secrets>(self)?;
        Ok(shell.write_file(path, bytes)?)
    }
}

impl ReadConfig for SecretsConfig {
    fn read(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = shell.current_dir().join(path);
        decode_yaml_repr::<zksync_protobuf_config::proto::secrets::Secrets>(&path, false)
    }
}
