use std::{
    fs,
    io::{Read, Write},
    mem::take,
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};
use sha2::{Digest, Sha256};
use sqruff_lib::{api::simple::get_simple_config, core::linter::core::Linter};
use walkdir::WalkDir;
use xshell::{cmd, Shell};
use zkstack_cli_config::EcosystemConfig;

use crate::commands::dev::messages::msg_file_is_not_formatted;

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

const SNAPSHOT_FILE: &str = ".format_sql_snapshot";

pub async fn calculate_fingerprint(code_root: &Path, dal_root: &PathBuf) -> Result<String> {
    let mut files: Vec<PathBuf> = WalkDir::new(&dal_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();

    files.sort(); // deterministic order

    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8 * 1024]; // 8 KiB buffer

    for path in files {
        // include the *relative* path, so a rename is detected
        hasher.update(path.strip_prefix(code_root)?.to_string_lossy().as_bytes());

        // stream file contents into the hash
        let mut f = fs::File::open(&path)?;
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }

    Ok(format!("{:x}", hasher.finalize()))
}

pub async fn format_sql(shell: Shell, check: bool) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(&shell)?;

    let code_root: &Path = &ecosystem.link_to_code;
    let dal_root: PathBuf = code_root.join("core/lib/dal");

    let fingerprint = calculate_fingerprint(code_root, &dal_root).await?;

    let snapshot_path = code_root.join(SNAPSHOT_FILE);
    if let Ok(prev) = fs::read_to_string(&snapshot_path) {
        if prev.trim() == fingerprint {
            // No changes detected — skip formatting.
            return Ok(());
        }
    }

    let output = cmd!(shell, "git -C {code_root} ls-files core/lib/dal").read()?;
    for file in output.lines() {
        if file.ends_with(".rs") {
            fmt_file(&shell, file, check)?;
        }
    }

    let new_fingerprint = calculate_fingerprint(code_root, &dal_root).await?;
    let mut f = fs::File::create(&snapshot_path)?;
    writeln!(f, "{new_fingerprint}")?;

    Ok(())
}
