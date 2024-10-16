use std::{collections::HashMap, net::TcpListener};

use anyhow::Context;
use clap::Parser;
use common::logger;
use config::EcosystemConfig;
use serde::Deserialize;
use serde_json::Value;
use xshell::Shell;

use crate::{
    commands::dev::messages::{
        MSG_API_CONFIG_NOT_FOUND_ERR, MSG_STATUS_PORTS_HELP, MSG_STATUS_URL_HELP,
    },
    messages::MSG_CHAIN_NOT_FOUND_ERR,
    utils::ports::EcosystemPortsScanner,
};

const DEFAULT_LINE_WIDTH: usize = 32;

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
    #[clap(subcommand)]
    pub subcommand: Option<StatusSubcommands>,
}

#[derive(Debug, Parser)]
pub enum StatusSubcommands {
    #[clap(about = MSG_STATUS_PORTS_HELP)]
    Ports,
}

struct BoxProperties {
    longest_line: usize,
    border: String,
    boxed_msg: Vec<String>,
}

impl BoxProperties {
    fn new(msg: &str) -> Self {
        let longest_line = msg
            .lines()
            .map(|line| line.len())
            .max()
            .unwrap_or(0)
            .max(DEFAULT_LINE_WIDTH);
        let width = longest_line + 2;
        let border = "─".repeat(width);
        let boxed_msg = msg
            .lines()
            .map(|line| format!("│ {:longest_line$} │", line))
            .collect();
        Self {
            longest_line,
            border,
            boxed_msg,
        }
    }
}

fn single_bordered_box(msg: &str) -> String {
    let properties = BoxProperties::new(msg);
    format!(
        "┌{}┐\n{}\n└{}┘\n",
        properties.border,
        properties.boxed_msg.join("\n"),
        properties.border
    )
}

fn bordered_boxes(msg1: &str, msg2: Option<&String>) -> String {
    if msg2.is_none() {
        return single_bordered_box(msg1);
    }

    let properties1 = BoxProperties::new(msg1);
    let properties2 = BoxProperties::new(msg2.unwrap());

    let max_lines = properties1.boxed_msg.len().max(properties2.boxed_msg.len());
    let header = format!("┌{}┐  ┌{}┐\n", properties1.border, properties2.border);
    let footer = format!("└{}┘  └{}┘\n", properties1.border, properties2.border);

    let empty_line1 = format!(
        "│ {:longest_line$} │",
        "",
        longest_line = properties1.longest_line
    );
    let empty_line2 = format!(
        "│ {:longest_line$} │",
        "",
        longest_line = properties2.longest_line
    );

    let boxed_info: Vec<String> = (0..max_lines)
        .map(|i| {
            let line1 = properties1.boxed_msg.get(i).unwrap_or(&empty_line1);
            let line2 = properties2.boxed_msg.get(i).unwrap_or(&empty_line2);
            format!("{}  {}", line1, line2)
        })
        .collect();

    format!("{}{}\n{}", header, boxed_info.join("\n"), footer)
}

fn print_status(health_check_url: String) -> anyhow::Result<()> {
    let client = reqwest::blocking::Client::new();
    let response = client.get(&health_check_url).send()?.text()?;

    let status_response: StatusResponse = serde_json::from_str(&response)
        .context(format!("Failed to parse response: {}", response))?;

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
                component_info.push_str(&format!("\n  - {}: {}", deslugified_key, value));
            }
        }

        if component.status.to_lowercase() != "ready" {
            not_ready_components.push(readable_name);
        }

        components.push(component_info);
    }

    components.sort_by(|a, b| {
        a.lines()
            .count()
            .cmp(&b.lines().count())
            .then_with(|| a.cmp(b))
    });

    for chunk in components.chunks(2) {
        components_info.push_str(&bordered_boxes(&chunk[0], chunk.get(1)));
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

fn is_port_open(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_err() || TcpListener::bind(("127.0.0.1", port)).is_err()
}

fn print_ports(shell: &Shell) -> anyhow::Result<()> {
    let ports = EcosystemPortsScanner::scan(shell)?;
    let grouped_ports = ports.group_by_file_path();

    let mut all_port_lines: Vec<String> = Vec::new();

    for (file_path, port_infos) in grouped_ports {
        let mut port_info_lines = String::new();

        for port_info in port_infos {
            let in_use_tag = if is_port_open(port_info.port) {
                " [OPEN]"
            } else {
                ""
            };

            port_info_lines.push_str(&format!(
                "  - {}{} > {}\n",
                port_info.port, in_use_tag, port_info.description
            ));
        }

        all_port_lines.push(format!("{}:\n{}", file_path, port_info_lines));
    }

    all_port_lines.sort_by(|a, b| {
        b.lines()
            .count()
            .cmp(&a.lines().count())
            .then_with(|| a.cmp(b))
    });

    let mut components_info = String::new();
    for chunk in all_port_lines.chunks(2) {
        components_info.push_str(&bordered_boxes(&chunk[0], chunk.get(1)));
    }

    logger::info(components_info);
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
    if let Some(StatusSubcommands::Ports) = args.subcommand {
        return print_ports(shell);
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

    print_status(health_check_url)
}
