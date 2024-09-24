use common::spinner::Spinner;
use xshell::Shell;

use super::lint_utils::{get_unignored_files, Target};
use anyhow::{bail, Result};

fn format_one_line_query(line: &str) -> String {
    line.to_string()
}

fn format_rust_string_query(built_query: &str, is_raw_string: bool) -> String {
    built_query.to_string()
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
            modified_file.push_str(&format_one_line_query(line));
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
                (line.ends_with("\",") || line.ends_with("\"")) && query_not_empty;
            built_query.push_str(line);
            built_query.push('\n');

            let line_end_is_not_escape = !line.ends_with("\\\"") && !line.ends_with("\\\",");
            if (is_raw_string && raw_string_query_ended)
                || (!is_raw_string && regular_string_query_ended && line_end_is_not_escape)
            {
                is_inside_query = false;
                let ended_with_comma = built_query.trim_end().ends_with(',');
                modified_file
                    .push_str(&format_rust_string_query(&built_query, is_raw_string).trim_end());
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
            bail!("File {} is not formatted", file_path);
        } else {
            shell.write_file(file_path, &modified_file)?;
        }
    }

    Ok(())
}

pub async fn format_sql(shell: Shell, check: bool) -> anyhow::Result<()> {
    let spinner = Spinner::new("Running SQL formatter");
    let rust_files = get_unignored_files(&shell, &Target::Rs)?;
    for file in rust_files {
        fmt_file(&shell, &file, check)?;
    }
    spinner.finish();
    Ok(())
}
