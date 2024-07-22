use crate::{cli::ProverCLIConfig, config};

pub async fn run(cfg: ProverCLIConfig) -> anyhow::Result<()> {
    let envfile = config::get_envfile()?;
    config::update_envfile(&envfile, "PLI__DB_URL", cfg.db_url.expose_str())?;
    Ok(())
}
