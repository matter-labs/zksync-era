//! CLI Command for printing configs.

use zksync_config::metadata::{ConfigMetadata, ParamMetadata};

const INDENT: &str = "    ";

fn print_parameter(param: &ParamMetadata, prefix: &str) {
    println!("{prefix}{}", param.name);
    if !param.aliases.is_empty() {
        let mut aliases_string = String::new();
        for (i, alias) in param.aliases.iter().enumerate() {
            aliases_string.push_str(alias);
            if i + 1 < param.aliases.len() {
                aliases_string.push_str(", ");
            }
        }
        println!("{INDENT}Aliases: {aliases_string}");
    }

    let ty = if let Some(kind) = param.base_type.kind() {
        format!("{kind} [Rust: {}]", param.ty.name_in_code())
    } else {
        param.ty.name_in_code().to_owned()
    };
    let default = if let Some(default) = &param.default_value {
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

pub(super) fn print_config(config_metadata: &ConfigMetadata, prefix: &str) {
    println!("{}\n", config_metadata.help);
    for param in &config_metadata.params {
        print_parameter(param, prefix);
        println!();
    }
}
