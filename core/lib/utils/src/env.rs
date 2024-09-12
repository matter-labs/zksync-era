use std::{
    path::{Path, PathBuf},
    str,
};

use anyhow::Context as _;
use once_cell::sync::OnceCell;

static WORKSPACE: OnceCell<Option<PathBuf>> = OnceCell::new();

/// Represents Cargo workspaces available in the repository.
#[derive(Debug, Clone, Copy)]
pub enum Workspace<'a> {
    /// Workspace was not found.
    /// Assumes that the code is running in a binary.
    /// Will use the current directory as a fallback.
    None,
    /// Root folder.
    Core(&'a Path),
    /// `prover` folder.
    Prover(&'a Path),
    /// `toolbox` folder.
    Toolbox(&'a Path),
}

impl Workspace<'static> {
    /// Find the location of the current workspace, if this code works in workspace
    /// then it will return the correct folder if, it's binary e.g. in docker container
    /// you have to use fallback to another directory
    /// The code has been inspired by `insta`
    /// `https://github.com/mitsuhiko/insta/blob/master/insta/src/env.rs`
    pub fn locate() -> Self {
        // Since `locate_workspace_inner()` should be deterministic, it makes little sense to call
        // `OnceCell::get_or_try_init()` here; the repeated calls are just as unlikely to succeed as the initial call.
        // Instead, we store `None` in the `OnceCell` if initialization failed.
        let path: Option<&'static Path> = WORKSPACE
            .get_or_init(|| {
                let result = locate_workspace_inner();
                // If the workspace is not found, we store `None` in the `OnceCell`.
                // It doesn't make sense to log it, since in most production cases the workspace
                // is not present.
                result.ok()
            })
            .as_deref();
        path.map_or(Self::None, Self::from)
    }
}

impl<'a> Workspace<'a> {
    const PROVER_DIRECTORY_NAME: &'static str = "prover";
    const TOOLBOX_DIRECTORY_NAME: &'static str = "zk_toolbox";

    /// Returns the path of the core workspace.
    /// For `Workspace::None`, considers the current directory to represent core workspace.
    pub fn core(self) -> PathBuf {
        match self {
            Self::None => PathBuf::from("."),
            Self::Core(path) => path.into(),
            Self::Prover(path) | Self::Toolbox(path) => path.parent().unwrap().into(),
        }
    }

    /// Returns the path of the `prover` workspace.
    pub fn prover(self) -> PathBuf {
        match self {
            Self::Prover(path) => path.into(),
            _ => self.core().join(Self::PROVER_DIRECTORY_NAME),
        }
    }

    /// Returns the path of the `zk_toolbox`` workspace.
    pub fn toolbox(self) -> PathBuf {
        match self {
            Self::Toolbox(path) => path.into(),
            _ => self.core().join(Self::TOOLBOX_DIRECTORY_NAME),
        }
    }
}

impl<'a> From<&'a Path> for Workspace<'a> {
    fn from(path: &'a Path) -> Self {
        if path.ends_with(Self::PROVER_DIRECTORY_NAME) {
            Self::Prover(path)
        } else if path.ends_with(Self::TOOLBOX_DIRECTORY_NAME) {
            Self::Toolbox(path)
        } else {
            Self::Core(path)
        }
    }
}

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

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    /// Will reset the pwd on drop.
    /// This is needed to make sure that even if the test fails, the env
    /// for other tests is left intact.
    struct PwdProtector(PathBuf);

    impl PwdProtector {
        fn new() -> Self {
            let pwd = std::env::current_dir().unwrap();
            Self(pwd)
        }
    }

    impl Drop for PwdProtector {
        fn drop(&mut self) {
            std::env::set_current_dir(self.0.clone()).unwrap();
        }
    }

    #[test]
    fn test_workspace_locate() {
        let _pwd_protector = PwdProtector::new();

        // Core.

        let workspace = Workspace::locate();
        assert_matches!(workspace, Workspace::Core(_));
        let core_path = workspace.core();
        // Check if prover and toolbox directories exist.
        assert!(workspace.prover().exists());
        assert_matches!(
            Workspace::from(workspace.prover().as_path()),
            Workspace::Prover(_)
        );
        assert!(workspace.toolbox().exists());
        assert_matches!(
            Workspace::from(workspace.toolbox().as_path()),
            Workspace::Toolbox(_)
        );

        // Prover.

        // We use `cargo-nextest` for running tests, which runs each test in parallel,
        // so we can safely alter the global env, assuming that we will restore it after
        // the test.
        std::env::set_current_dir(workspace.prover()).unwrap();
        let workspace_path = locate_workspace_inner().unwrap();
        let workspace = Workspace::from(workspace_path.as_path());
        assert_matches!(workspace, Workspace::Prover(_));
        let prover_path = workspace.prover();
        assert_eq!(workspace.core(), core_path);
        assert_matches!(
            Workspace::from(workspace.core().as_path()),
            Workspace::Core(_)
        );
        assert!(workspace.toolbox().exists());
        assert_matches!(
            Workspace::from(workspace.toolbox().as_path()),
            Workspace::Toolbox(_)
        );

        // Toolbox.
        std::env::set_current_dir(workspace.toolbox()).unwrap();
        let workspace_path = locate_workspace_inner().unwrap();
        let workspace = Workspace::from(workspace_path.as_path());
        assert_matches!(workspace, Workspace::Toolbox(_));
        assert_eq!(workspace.core(), core_path);
        assert_matches!(
            Workspace::from(workspace.core().as_path()),
            Workspace::Core(_)
        );
        assert_eq!(workspace.prover(), prover_path);
        assert_matches!(
            Workspace::from(workspace.prover().as_path()),
            Workspace::Prover(_)
        );
    }
}
