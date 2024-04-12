use std::path::PathBuf;

use once_cell::sync::OnceCell;

static WORKSPACE: OnceCell<PathBuf> = OnceCell::new();

/// Find the location of the current workspace, if this code works in workspace
/// then it will return the correct folder if, it's binary e.g. in docker container
/// you have to use fallback to another directory
/// The code has been inspired by `insta`
/// `https://github.com/mitsuhiko/insta/blob/master/insta/src/env.rs`
pub fn locate_workspace() -> Option<PathBuf> {
    let workspace = WORKSPACE.get().cloned();
    if let Some(workspace) = workspace {
        return Some(workspace);
    };

    let output = std::process::Command::new(
        std::env::var("CARGO")
            .ok()
            .unwrap_or_else(|| "cargo".to_string()),
    )
    .arg("locate-project")
    .arg("--workspace")
    .output()
    .map_err(|err| {
        tracing::trace!("Can't find workspace location: {}", err);
        err
    })
    .ok()?;

    let Some(root) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .map_err(|err| {
            tracing::warn!("Parsing `cargo locate-project` error: {}", err);
            err
        })
        .ok()?
        .get("root")
        .cloned()
    else {
        tracing::warn!("root doesn't exist in output from `cargo locate-project`");
        return None;
    };

    let serde_json::Value::String(root) = root else {
        tracing::warn!("root is not a string");
        return None;
    };

    let val = PathBuf::from(root).parent()?.to_path_buf();
    Some(WORKSPACE.get_or_init(|| val).clone())
}
