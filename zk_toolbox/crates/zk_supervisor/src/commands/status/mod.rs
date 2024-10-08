use std::collections::HashMap;

use anyhow::Context;
use clap::Parser;
use common::{cmd::Cmd, logger};
use config::EcosystemConfig;
use serde::Deserialize;
use serde_json::Value;
use xshell::{cmd, Shell};

use crate::messages::{MSG_API_CONFIG_NOT_FOUND_ERR, MSG_CHAIN_NOT_FOUND_ERR};

#[derive(Deserialize, Debug)]
struct StatusResponse {
    status: String,
    components: HashMap<String, Component>,
}

#[derive(Deserialize, Debug)]
struct Component {
    status: String,
    details: Option<Value>,
}

#[derive(Debug, Parser)]
pub struct StatusArgs {
    #[clap(long, short = 'u')]
    pub url: Option<String>,
    #[clap(long, short = 'p')]
    pub ports: bool,
}

fn get_status(shell: &Shell, health_check_url: String) -> anyhow::Result<()> {
    let response = Cmd::new(cmd!(shell, "curl {health_check_url}")).run_with_output()?;
    let response = String::from_utf8(response.stdout)?;

    logger::debug(format!("Response: {}", response.clone()));
    let status_response: StatusResponse = serde_json::from_str(&response)?;
    logger::debug(format!("Status: {}", status_response.status));
    for (component, status) in status_response.components {
        logger::debug(format!("Component: {}", component));
        logger::debug(format!("Status: {}", status.status));
        if let Some(details) = status.details {
            logger::debug(format!("Details: {}", details));
        }
    }

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
