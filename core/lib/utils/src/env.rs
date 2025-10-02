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
    /// Root folder.
    Root,
    /// `core` folder.
    Core(&'a Path),
    /// `prover` folder.
    Prover(&'a Path),
    /// ZK Stack CLI folder.
    ZkStackCli(&'a Path),
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
        path.map_or(Self::Root, Self::from)
    }
}

impl<'a> Workspace<'a> {
    const CORE_DIRECTORY_NAME: &'static str = "core";
    const PROVER_DIRECTORY_NAME: &'static str = "prover";
    const ZKSTACK_CLI_DIRECTORY_NAME: &'static str = "zkstack_cli";

    /// Returns the path of the repository root.
    pub fn root(self) -> PathBuf {
        match self {
            Self::Root => PathBuf::from("."),
            Self::Core(path) | Self::Prover(path) | Self::ZkStackCli(path) => {
                path.parent().unwrap().into()
            }
        }
    }

    /// Returns the path of the `core` workspace.
    pub fn core(self) -> PathBuf {
        match self {
            Self::Core(path) => path.into(),
            _ => self.root().join(Self::CORE_DIRECTORY_NAME),
        }
    }

    /// Returns the path of the `prover` workspace.
    pub fn prover(self) -> PathBuf {
        match self {
            Self::Prover(path) => path.into(),
            _ => self.root().join(Self::PROVER_DIRECTORY_NAME),
        }
    }

    /// Returns the path of the ZK Stack CLI workspace.
    pub fn zkstack_cli(self) -> PathBuf {
        match self {
            Self::ZkStackCli(path) => path.into(),
            _ => self.root().join(Self::ZKSTACK_CLI_DIRECTORY_NAME),
        }
    }
}

impl<'a> From<&'a Path> for Workspace<'a> {
    fn from(path: &'a Path) -> Self {
        if path.ends_with(Self::PROVER_DIRECTORY_NAME) {
            Self::Prover(path)
        } else if path.ends_with(Self::ZKSTACK_CLI_DIRECTORY_NAME) {
            Self::ZkStackCli(path)
        } else if path.ends_with(Self::CORE_DIRECTORY_NAME) {
            Self::Core(path)
        } else {
            Self::Root
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
        // Check if prover and ZK Stack CLI directories exist.
        assert!(workspace.prover().exists());
        assert_matches!(
            Workspace::from(workspace.prover().as_path()),
            Workspace::Prover(_)
        );
        assert!(workspace.zkstack_cli().exists());
        assert_matches!(
            Workspace::from(workspace.zkstack_cli().as_path()),
            Workspace::ZkStackCli(_)
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
        assert!(workspace.zkstack_cli().exists());
        assert_matches!(
            Workspace::from(workspace.zkstack_cli().as_path()),
            Workspace::ZkStackCli(_)
        );

        // ZK Stack CLI
        std::env::set_current_dir(workspace.zkstack_cli()).unwrap();
        let workspace_path = locate_workspace_inner().unwrap();
        let workspace = Workspace::from(workspace_path.as_path());
        assert_matches!(workspace, Workspace::ZkStackCli(_));
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
