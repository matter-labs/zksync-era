use std::collections::HashMap;

use anyhow::Context;
use args::{StatusArgs, StatusSubcommands};
use draw::{bordered_boxes, format_port_info};
use serde::Deserialize;
use serde_json::Value;
use utils::deslugify;
use xshell::Shell;
use zkstack_cli_common::logger;

use crate::{
    commands::dev::messages::{
        msg_failed_parse_response, msg_not_ready_components, msg_system_status,
        MSG_ALL_COMPONENTS_READY, MSG_COMPONENTS, MSG_SOME_COMPONENTS_NOT_READY,
    },
    utils::ports::EcosystemPortsScanner,
};

pub mod args;
mod draw;
mod utils;

const STATUS_READY: &str = "ready";

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

fn print_status(health_check_url: String) -> anyhow::Result<()> {
    let client = reqwest::blocking::Client::new();
    let response = client.get(&health_check_url).send()?.text()?;

    let status_response: StatusResponse =
        serde_json::from_str(&response).context(msg_failed_parse_response(&response))?;

    if status_response.status.to_lowercase() == STATUS_READY {
        logger::success(msg_system_status(&status_response.status));
    } else {
        logger::warn(msg_system_status(&status_response.status));
    }

    let mut components_info = String::from(MSG_COMPONENTS);
    let mut components = Vec::new();
    let mut not_ready_components = Vec::new();

    for (component_name, component) in status_response.components {
        let readable_name = deslugify(&component_name);
        let mut component_info = format!("{}:\n  - Status: {}", readable_name, component.status);

        if let Some(details) = &component.details {
            for (key, value) in details.as_object().unwrap() {
                component_info.push_str(&format!("\n  - {}: {}", deslugify(key), value));
            }
        }

        if component.status.to_lowercase() != STATUS_READY {
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
        logger::outro(MSG_ALL_COMPONENTS_READY);
    } else {
        logger::warn(MSG_SOME_COMPONENTS_NOT_READY);
        logger::outro(msg_not_ready_components(&not_ready_components.join(", ")));
    }

    Ok(())
}

fn print_ports(shell: &Shell) -> anyhow::Result<()> {
    let ports = EcosystemPortsScanner::scan(shell, None)?;
    let grouped_ports = ports.group_by_file_path();

    let mut all_port_lines: Vec<String> = Vec::new();

    for (file_path, port_infos) in grouped_ports {
        let mut port_info_lines = String::new();

        for port_info in port_infos {
            port_info_lines.push_str(&format_port_info(&port_info));
        }

        all_port_lines.push(format!("{}:\n{}", file_path, port_info_lines));
    }

    all_port_lines.sort_by(|a, b| {
        b.lines()
            .count()
            .cmp(&a.lines().count())
            .then_with(|| a.cmp(b))
    });

    let mut components_info = String::from("Ports:\n");
    for chunk in all_port_lines.chunks(2) {
        components_info.push_str(&bordered_boxes(&chunk[0], chunk.get(1)));
    }

    logger::info(components_info);
    Ok(())
}

pub async fn run(shell: &Shell, args: StatusArgs) -> anyhow::Result<()> {
    if let Some(StatusSubcommands::Ports) = args.subcommand {
        return print_ports(shell);
    }

    let health_check_url = args.get_url(shell).await?;

    print_status(health_check_url)
}
