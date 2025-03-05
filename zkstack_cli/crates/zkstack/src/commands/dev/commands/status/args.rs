use anyhow::Context;
use clap::Parser;
use xshell::Shell;
use zkstack_cli_config::EcosystemConfig;

use crate::{
    commands::dev::messages::{MSG_STATUS_PORTS_HELP, MSG_STATUS_URL_HELP},
    messages::MSG_CHAIN_NOT_FOUND_ERR,
};

#[derive(Debug, Parser)]
pub enum StatusSubcommands {
    #[clap(about = MSG_STATUS_PORTS_HELP)]
    Ports,
}

#[derive(Debug, Parser)]
pub struct StatusArgs {
    #[clap(long, short = 'u', help = MSG_STATUS_URL_HELP)]
    pub url: Option<String>,
    #[clap(subcommand)]
    pub subcommand: Option<StatusSubcommands>,
}

impl StatusArgs {
    pub async fn get_url(&self, shell: &Shell) -> anyhow::Result<String> {
        if let Some(url) = &self.url {
            Ok(url.clone())
        } else {
            let ecosystem = EcosystemConfig::from_file(shell)?;
            let chain = ecosystem
                .load_current_chain()
                .context(MSG_CHAIN_NOT_FOUND_ERR)?;
            let general_config = chain.get_general_config().await?;
            general_config.healthcheck_url()
        }
    }
}
