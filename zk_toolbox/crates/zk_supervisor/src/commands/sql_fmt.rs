use common::spinner::Spinner;
use xshell::Shell;

use super::lint_utils::{get_unignored_files, Target};

fn fmt_file(shell: &Shell, file_path: &str, check: bool) -> anyhow::Result<()> {
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
