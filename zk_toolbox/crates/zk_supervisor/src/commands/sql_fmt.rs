use anyhow::{bail, Context, Result};
use common::spinner::Spinner;
use regex::Regex;
use xshell::Shell;

use super::lint_utils::{get_unignored_files, IgnoredData, Target};
use crate::messages::{
    msg_file_is_not_formatted, MSG_FAILED_TO_DETERMINE_BASE_INDENT,
    MSG_FAILED_TO_FIND_END_OF_REGULAR_STRING_QUERY,
    MSG_FAILED_TO_FIND_START_OF_REGULAR_STRING_QUERY, MSG_RUNNING_SQL_FMT_SPINNER,
};

fn format_query(query: &str) -> anyhow::Result<String> {
    let options = sqlformat::FormatOptions {
        indent: sqlformat::Indent::Spaces(4),
        uppercase: true,
        lines_between_queries: 1,
    };

    let mut formatted_query = sqlformat::format(query, &sqlformat::QueryParams::None, options);

    // Replace certain keywords with lowercase versions
    let keywords = vec![
        "STORAGE",
        "TIMESTAMP",
        "INPUT",
        "DATA",
        "ZONE",
        "VALUE",
        "DEPTH",
        "KEY",
        "KEYS",
        "STATUS",
        "EVENTS",
    ];
    for keyword in keywords {
        let regex = Regex::new(&format!(r"\b{}\b", keyword))?;
        formatted_query = regex
            .replace_all(&formatted_query, keyword.to_lowercase())
            .to_string();
    }

    // Remove minimum indent from the formatted query
    let formatted_lines: Vec<&str> = formatted_query.lines().collect();
    let min_indent = formatted_lines
        .iter()
        .filter_map(|line| line.find(|c: char| !c.is_whitespace()))
        .min()
        .unwrap_or(0);
    Ok(formatted_query
        .lines()
        .map(|line| line[min_indent..].to_string())
        .collect::<Vec<String>>()
        .join("\n"))
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

fn format_one_line_query(line: &str) -> anyhow::Result<String> {
    let is_raw_string = line.contains("sqlx::query!(r");

    let query_start = if is_raw_string {
        line.find(r#"r#""#)
            .context(MSG_FAILED_TO_FIND_START_OF_REGULAR_STRING_QUERY)?
    } else {
        line.find('"')
            .context(MSG_FAILED_TO_FIND_START_OF_REGULAR_STRING_QUERY)?
    };

    let base_indent = line
        .find(|c: char| !c.is_whitespace())
        .context(MSG_FAILED_TO_DETERMINE_BASE_INDENT)?
        + 4;

    let prefix = &line[..query_start];

    let query_end = if is_raw_string {
        line.find(r#""#)
            .context(MSG_FAILED_TO_FIND_END_OF_REGULAR_STRING_QUERY)?
            + 2
    } else {
        line[1..]
            .find('"')
            .context(MSG_FAILED_TO_FIND_END_OF_REGULAR_STRING_QUERY)?
            + 3
    };

    let suffix = &line[query_end..];
    let query = &line[query_start..query_end];

    let mut formatted_query = format_rust_string_query(query, is_raw_string)?;
    formatted_query = add_indent(&formatted_query, base_indent);

    Ok(format!("{}\n{}\n{}", prefix, formatted_query, suffix))
}

fn fmt_file(shell: &Shell, file_path: &str, check: bool) -> Result<()> {
    let content = shell.read_file(file_path)?;
    let mut modified_file = String::new();

    let mut lines_to_query: Option<usize> = None;
    let mut is_inside_query = false;
    let mut is_raw_string = false;
    let mut built_query = String::new();

    for line in content.lines() {
        if line.contains("sqlx::query!(\"") || line.contains("sqlx::query!(r") {
            modified_file.push_str(&format_one_line_query(line)?);
            modified_file.push('\n');
            continue;
        }

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
