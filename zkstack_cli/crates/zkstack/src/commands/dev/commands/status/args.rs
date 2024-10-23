use anyhow::Context;
use clap::Parser;
use config::EcosystemConfig;
use xshell::Shell;

use crate::{
    commands::dev::messages::{
        MSG_API_CONFIG_NOT_FOUND_ERR, MSG_STATUS_PORTS_HELP, MSG_STATUS_URL_HELP,
    },
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
    pub fn get_url(&self, shell: &Shell) -> anyhow::Result<String> {
        if let Some(url) = &self.url {
            Ok(url.clone())
        } else {
            let ecosystem = EcosystemConfig::from_file(shell)?;
            let chain = ecosystem
                .load_current_chain()
                .context(MSG_CHAIN_NOT_FOUND_ERR)?;
            let general_config = chain.get_general_config()?;
            let health_check_port = general_config
                .api_config
                .context(MSG_API_CONFIG_NOT_FOUND_ERR)?
                .healthcheck
                .port;
            Ok(format!("http://localhost:{}/health", health_check_port))
        }
    }
}
