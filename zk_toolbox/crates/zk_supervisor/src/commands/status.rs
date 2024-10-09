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

fn bordered_box(msg: &str) -> String {
    let longest_line = msg.lines().map(|line| line.len()).max().unwrap_or(0);
    let width = longest_line + 2;
    let border = "─".repeat(width);
    let boxed_msg = msg
        .lines()
        .map(|line| format!("│ {:longest_line$} │", line))
        .collect::<Vec<_>>()
        .join("\n");
    format!("┌{}┐\n{}\n└{}┘\n", border, boxed_msg, border)
}

fn two_bordered_boxes(msg1: &str, msg2: &str) -> String {
    let longest_line1 = msg1.lines().map(|line| line.len()).max().unwrap_or(0);
    let longest_line2 = msg2.lines().map(|line| line.len()).max().unwrap_or(0);
    let width1 = longest_line1 + 2;
    let width2 = longest_line2 + 2;

    let border1 = "─".repeat(width1);
    let border2 = "─".repeat(width2);

    let boxed_msg1: Vec<String> = msg1
        .lines()
        .map(|line| format!("│ {:longest_line1$} │", line))
        .collect();

    let boxed_msg2: Vec<String> = msg2
        .lines()
        .map(|line| format!("│ {:longest_line2$} │", line))
        .collect();

    let max_lines = boxed_msg1.len().max(boxed_msg2.len());

    let header = format!("┌{}┐  ┌{}┐\n", border1, border2);
    let footer = format!("└{}┘  └{}┘\n", border1, border2);
    let empty_line1 = format!("│ {:longest_line1$} │", "");
    let empty_line2 = format!("│ {:longest_line2$} │", "");

    let boxed_info: Vec<String> = (0..max_lines)
        .map(|i| {
            let line1 = boxed_msg1.get(i).unwrap_or(&empty_line1);
            let line2 = boxed_msg2.get(i).unwrap_or(&empty_line2);
            format!("{}  {}", line1, line2)
        })
        .collect();

    format!("{}{}\n{}", header, boxed_info.join("\n"), footer)
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

    let mut components_info = String::from("Components:\n");
    let mut components = Vec::new();
    let mut not_ready_components = Vec::new();

    for (component_name, component) in status_response.components {
        let readable_name = deslugify(&component_name);
        let mut component_info = format!("{}:\n  - Status: {}", readable_name, component.status);

        if let Some(details) = &component.details {
            for (key, value) in details.as_object().unwrap() {
                let deslugified_key = deslugify(key);
                component_info.push_str(&format!("\n    - {}: {}", deslugified_key, value));
            }
        }

        if component.status.to_lowercase() != "ready" {
            not_ready_components.push(readable_name);
        }

        components.push(component_info);
    }

    // Add components boxes to the components info string
    // grouped by 2 components per line
    for chunk in components.chunks(2) {
        if let Some(component) = chunk.get(1) {
            components_info.push_str(&two_bordered_boxes(chunk.first().unwrap(), component));
        } else {
            components_info.push_str(&bordered_box(chunk.first().unwrap()));
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
