use std::{
    path::{Path, PathBuf},
    str,
};

use anyhow::Context as _;
use once_cell::sync::OnceCell;

static WORKSPACE: OnceCell<Option<PathBuf>> = OnceCell::new();

fn locate_workspace_inner() -> anyhow::Result<PathBuf> {
    let output = std::process::Command::new(
        std::env::var("CARGO")
            .ok()
            .unwrap_or_else(|| "cargo".to_string()),
    )
    .arg("locate-project")
    .arg("--workspace")
    .output()
    .context("Can't find Cargo workspace location")?;

    let output =
        serde_json::from_slice::<serde_json::Value>(&output.stdout).with_context(|| {
            format!(
                "Error parsing `cargo locate-project` output {}",
                str::from_utf8(&output.stdout).unwrap_or("(non-utf8 output)")
            )
        })?;
    let root = output.get("root").with_context(|| {
        format!("root doesn't exist in output from `cargo locate-project` {output:?}")
    })?;

    let serde_json::Value::String(root) = root else {
        return Err(anyhow::anyhow!("`root` is not a string: {root:?}"));
    };
    let root_path = PathBuf::from(root);
    Ok(root_path
        .parent()
        .with_context(|| format!("`root` path doesn't have a parent: {}", root_path.display()))?
        .to_path_buf())
}

/// Find the location of the current workspace, if this code works in workspace
/// then it will return the correct folder if, it's binary e.g. in docker container
/// you have to use fallback to another directory
/// The code has been inspired by `insta`
/// `https://github.com/mitsuhiko/insta/blob/master/insta/src/env.rs`
pub fn locate_workspace() -> Option<&'static Path> {
    // Since `locate_workspace_inner()` should be deterministic, it makes little sense to call
    // `OnceCell::get_or_try_init()` here; the repeated calls are just as unlikely to succeed as the initial call.
    // Instead, we store `None` in the `OnceCell` if initialization failed.
    WORKSPACE
        .get_or_init(|| {
            let result = locate_workspace_inner();
            if result.is_err() {
                // `get_or_init()` is guaranteed to call the provided closure once per `OnceCell`;
                // i.e., we won't spam logs here.
                tracing::info!(
                    "locate_workspace() failed. You are using an already compiled version"
                );
            }
            result.ok()
        })
        .as_deref()
}

/// Returns [`locate_workspace()`] output with the "." fallback.
pub fn workspace_dir_or_current_dir() -> &'static Path {
    locate_workspace().unwrap_or_else(|| Path::new("."))
}
