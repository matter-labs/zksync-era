use std::mem::take;

use anyhow::{bail, Result};
use common::spinner::Spinner;
use sqruff_lib::{api::simple::get_simple_config, core::linter::core::Linter};
use xshell::Shell;

use super::lint_utils::{get_unignored_files, IgnoredData, Target};
use crate::messages::{msg_file_is_not_formatted, MSG_RUNNING_SQL_FMT_SPINNER};

fn format_query(query: &str) -> anyhow::Result<String> {
    let exclude_rules = vec!["LT12".to_string()]; // avoid adding newline before `$` character
    let cfg = get_simple_config(Some("postgres".into()), None, Some(exclude_rules), None).unwrap();
    let mut linter = Linter::new(cfg, None, None);
    let mut result = linter.lint_string_wrapped(query, None, true);
    let formatted_query = take(&mut result.paths[0].files[0]).fix_string();
    // Remove first empty line
    let formatted_query = formatted_query
        .lines()
        .skip(1)
        .collect::<Vec<&str>>()
        .join("\n");

    Ok(formatted_query)
}

fn extract_query_from_rust_string(query: &str, is_raw: bool) -> String {
    let mut query = query.trim().to_string();
    if query.ends_with(',') {
        query.pop();
    }
    // Removing quotes
    if !is_raw {
        query = query[1..query.len() - 1].to_string();
    } else {
        query = query[3..query.len() - 2].to_string();
    }

    // Remove all escape characters
    if !is_raw {
        query = query.replace(r#"\""#, "\"");
    }

    query
}

fn embed_text_inside_rust_string(query: &str) -> String {
    format!("r#\"\n{}\n\"#", query)
}

fn add_indent(query: &str, indent: usize) -> String {
    query
        .lines()
        .map(|line| format!("{:indent$}{}", "", line))
        .collect::<Vec<String>>()
        .join("\n")
}

fn format_rust_string_query(query: &str, is_raw: bool) -> anyhow::Result<String> {
    let base_indent = query.find(|c: char| !c.is_whitespace()).unwrap_or(0);
    let raw_query = extract_query_from_rust_string(query, is_raw);
    let formatted_query = format_query(&raw_query)?;
    let reconstructed_rust_string = embed_text_inside_rust_string(&formatted_query);
    Ok(add_indent(&reconstructed_rust_string, base_indent))
}

fn fmt_file(shell: &Shell, file_path: &str, check: bool) -> Result<()> {
    let content = shell.read_file(file_path)?;
    let mut modified_file = String::new();

    let mut lines_to_query: Option<usize> = None;
    let mut is_inside_query = false;
    let mut is_raw_string = false;
    let mut built_query = String::new();

    for line in content.lines() {
        if line.ends_with("sqlx::query!(") {
            lines_to_query = Some(1);
            is_raw_string = false;
            built_query.clear();
        } else if line.ends_with("sqlx::query_as!(") {
            lines_to_query = Some(2);
            is_raw_string = false;
            built_query.clear();
        }

        if let Some(ref mut count) = lines_to_query {
            if *count == 0 {
                is_inside_query = true;
                lines_to_query = None;
                if line.contains("r#\"") {
                    is_raw_string = true;
                }
            } else {
                *count -= 1;
            }
        }

        if is_inside_query {
            let query_not_empty = !built_query.is_empty() || line.trim().len() > 1;
            let raw_string_query_ended = line.ends_with("\"#,") || line.ends_with("\"#");
            let regular_string_query_ended =
                (line.ends_with("\",") || line.ends_with('"')) && query_not_empty;
            built_query.push_str(line);
            built_query.push('\n');

            let line_end_is_not_escape = !line.ends_with("\\\"") && !line.ends_with("\\\",");
            if (is_raw_string && raw_string_query_ended)
                || (!is_raw_string && regular_string_query_ended && line_end_is_not_escape)
            {
                is_inside_query = false;
                let ended_with_comma = built_query.trim_end().ends_with(',');
                modified_file
                    .push_str(format_rust_string_query(&built_query, is_raw_string)?.trim_end());
                if ended_with_comma {
                    modified_file.push(',');
                }
                modified_file.push('\n');
            }
        } else {
            modified_file.push_str(line);
            modified_file.push('\n');
        }
    }

    if content != modified_file {
        if check {
            bail!(msg_file_is_not_formatted(file_path));
        } else {
            shell.write_file(file_path, &modified_file)?;
        }
    }

    Ok(())
}

pub async fn format_sql(shell: Shell, check: bool) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_RUNNING_SQL_FMT_SPINNER);
    let ignored_data = Some(IgnoredData {
        files: vec![],
        dirs: vec!["zk_toolbox".to_string()],
    });
    let rust_files = get_unignored_files(&shell, &Target::Rs, ignored_data)?;
    for file in rust_files {
        fmt_file(&shell, &file, check)?;
    }
    spinner.finish();
    Ok(())
}
