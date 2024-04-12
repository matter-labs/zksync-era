use std::{path::PathBuf, sync::Mutex};

static WORKSPACE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Find the location of the current workspace, if this code works in workspace
/// then it will return the correct folder if, it's prebuild binary e.g. in docker container
/// you have to use fallback to another directory
/// The code has been inspired by insta
/// https://github.com/mitsuhiko/insta/blob/74f3806b53bea6a4a6c16034e16f317a6dd4eea7/insta/src/env.rs#L369
pub fn locate_workspace() -> Option<PathBuf> {
    let mut workspace = WORKSPACE.lock().unwrap_or_else(|x| x.into_inner());
    if let Some(workspace) = workspace.clone() {
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
    .ok()?;
    let root = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .ok()?
        .get("root")
        .cloned()?;

    let serde_json::Value::String(root) = root else {
        return None;
    };
    let val = Some(PathBuf::from(root).parent()?.to_path_buf());
    *workspace = val;
    workspace.clone()
}
