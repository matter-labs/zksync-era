//! CLI Command for printing configs.

use zksync_config::metadata::{ConfigMetadata, ParamMetadata};

const INDENT: &str = "    ";

fn print_parameter(param: &ParamMetadata, prefix: &str) {
    println!("{prefix}{}", param.name);
    for &alias in param.aliases {
        println!("{prefix}{alias}");
    }

    let ty = if let Some(kind) = param.base_type.kind() {
        format!("{kind} [Rust: {}]", param.ty.name_in_code())
    } else {
        param.ty.name_in_code().to_owned()
    };
    let default = if let Some(default) = param.default_value() {
        format!(", default: {default:?}")
    } else {
        String::new()
    };
    let unit = if let Some(unit) = &param.unit {
        format!(" [unit: {unit}]")
    } else {
        String::new()
    };
    println!("{INDENT}Type: {ty}{default}{unit}");

    if !param.help.is_empty() {
        for line in param.help.lines() {
            println!("{INDENT}{line}");
        }
    }
}

pub(super) fn print_config(config_metadata: &ConfigMetadata, prefix: &str, filter: Option<&str>) {
    let filtered_params: Vec<_> = if let Some(filter) = filter {
        config_metadata
            .params
            .iter()
            .filter(|param| param.name.contains(filter) || param.help.contains(filter))
            .collect()
    } else {
        config_metadata.params.iter().collect()
    };
    if filtered_params.is_empty() {
        return;
    }

    println!("{}\n", config_metadata.help);
    for param in filtered_params {
        print_parameter(param, prefix);
        println!();
    }
}
