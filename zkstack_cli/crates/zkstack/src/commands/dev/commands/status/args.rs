use clap::Parser;
use xshell::Shell;
use zkstack_cli_config::ZkStackConfig;

use crate::commands::dev::messages::{MSG_STATUS_PORTS_HELP, MSG_STATUS_URL_HELP};

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
            let chain = ZkStackConfig::current_chain(shell)?;
            let general_config = chain.get_general_config().await?;
            general_config.healthcheck_url()
        }
    }
}
