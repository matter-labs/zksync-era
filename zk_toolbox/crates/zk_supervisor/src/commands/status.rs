use std::collections::HashMap;

use anyhow::Context;
use clap::Parser;
use common::{cmd::Cmd, logger};
use config::EcosystemConfig;
use serde::Deserialize;
use serde_json::Value;
use xshell::{cmd, Shell};

use crate::messages::{MSG_API_CONFIG_NOT_FOUND_ERR, MSG_CHAIN_NOT_FOUND_ERR, MSG_STATUS_URL_HELP};

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
    #[clap(long, short = 'u', help = MSG_STATUS_URL_HELP)]
    pub url: Option<String>,
}

fn print_status(shell: &Shell, health_check_url: String) -> anyhow::Result<()> {
    let response = Cmd::new(cmd!(shell, "curl {health_check_url}")).run_with_output()?;
    let response = String::from_utf8(response.stdout)?;

    let status_response: StatusResponse = serde_json::from_str(&response)?;

    if status_response.status.to_lowercase() == "ready" {
        logger::success(format!("System Status: {}\n", status_response.status));
    } else {
        logger::warn(format!("System Status: {}\n", status_response.status));
    }

    let mut components_info = String::from("Components:");

    let mut not_ready_components = Vec::new();

    for (component_name, component) in status_response.components {
        let readable_name = deslugify(&component_name);
        let mut component_info =
            format!("\n  {}:\n    - Status: {}", readable_name, component.status);

        if let Some(details) = &component.details {
            for (key, value) in details.as_object().unwrap() {
                let deslugified_key = deslugify(key);
                component_info.push_str(&format!("\n    - {}: {}", deslugified_key, value));
            }
        }

        components_info.push_str(&component_info);

        if component.status.to_lowercase() != "ready" {
            not_ready_components.push(readable_name);
        }
    }

    logger::info(components_info);

    if not_ready_components.is_empty() {
        logger::outro("Overall System Status: All components operational and ready.");
    } else {
        logger::warn("Overall System Status: Some components are not ready.");
        logger::outro(format!(
            "Not Ready Components: {}",
            not_ready_components.join(", ")
        ));
    }

    Ok(())
}

fn deslugify(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let capitalized = first.to_uppercase().collect::<String>() + chars.as_str();
                    match capitalized.as_str() {
                        "Http" => "HTTP".to_string(),
                        "Api" => "API".to_string(),
                        "Ws" => "WS".to_string(),
                        _ => capitalized,
                    }
                }
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

pub async fn run(shell: &Shell, args: StatusArgs) -> anyhow::Result<()> {
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

    print_status(shell, health_check_url)
}
