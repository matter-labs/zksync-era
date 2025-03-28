use zkstack_cli_common::db::DatabaseConfig;

use crate::raw::PatchedConfig;

pub fn set_server_database(
    secrets: &mut PatchedConfig,
    server_db_config: &DatabaseConfig,
) -> anyhow::Result<()> {
    secrets.insert(
        "database.server_url",
        server_db_config.full_url().to_string(),
    )
}

pub fn set_prover_database(
    secrets: &mut PatchedConfig,
    prover_db_config: &DatabaseConfig,
) -> anyhow::Result<()> {
    secrets.insert(
        "database.prover_url",
        prover_db_config.full_url().to_string(),
    )
}

pub fn set_l1_rpc_url(secrets: &mut PatchedConfig, l1_rpc_url: String) -> anyhow::Result<()> {
    secrets.insert("l1.l1_rpc_url", l1_rpc_url)
}
