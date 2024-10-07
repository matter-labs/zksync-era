use anyhow::Context;
use clap::Parser;
use config::EcosystemConfig;
use xshell::Shell;

use crate::messages::{MSG_API_CONFIG_NOT_FOUND_ERR, MSG_CHAIN_NOT_FOUND_ERR};

#[derive(Debug, Parser)]
pub struct StatusArgs {
    #[clap(long, short = 'u')]
    pub url: Option<String>,
    #[clap(long, short = 'p')]
    pub ports: bool,
}

fn get_status(shell: &Shell, health_check_url: String) -> anyhow::Result<()> {
    Ok(())
}

fn get_ports(shell: &Shell) -> anyhow::Result<()> {
    Ok(())
}

pub async fn run(shell: &Shell, args: StatusArgs) -> anyhow::Result<()> {
    if args.ports {
        return get_ports(shell);
    }

    let health_check_url = if let Some(url) = args.url {
        url
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
        format!("http://localhost:{}/health", health_check_port)
    };

    get_status(shell, health_check_url)
}
