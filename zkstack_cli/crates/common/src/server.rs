use std::{ffi::OsStr, path::PathBuf};

use anyhow::Context;
use xshell::{cmd, Shell};

use crate::{cmd::Cmd, github::GitHubTagFetcher, PromptSelect};

/// Allows to perform server operations.
#[derive(Debug)]
pub struct Server {
    components: Option<Vec<String>>,
    code_path: PathBuf,
    uring: bool,
}

/// Possible server modes.
#[derive(Debug)]
pub enum ServerMode {
    Normal,
    Genesis,
}

/// Possible execution modes.
#[derive(Debug, Default)]
pub enum ExecutionMode {
    #[default]
    Release,
    Debug,
    Docker,
}

impl Server {
    /// Creates a new instance of the server.
    pub fn new(components: Option<Vec<String>>, code_path: PathBuf, uring: bool) -> Self {
        Self {
            components,
            code_path,
            uring,
        }
    }

    /// Runs the server.
    #[allow(clippy::too_many_arguments)]
    pub async fn run<P>(
        &self,
        shell: &Shell,
        execution_mode: ExecutionMode,
        server_mode: ServerMode,
        configs_folder: P,
        chains_folder: P,
        genesis_path: P,
        wallets_path: P,
        general_path: P,
        secrets_path: P,
        contracts_path: P,
        mut additional_args: Vec<String>,
        tag: Option<String>,
    ) -> anyhow::Result<()>
    where
        P: AsRef<OsStr>,
    {
        let _dir_guard = shell.push_dir(&self.code_path);

        if let Some(components) = self.components() {
            additional_args.push(format!("--components={}", components))
        }
        if let ServerMode::Genesis = server_mode {
            additional_args.push("--genesis".to_string());
        }

        let uring = self.uring.then_some("--features=rocksdb/io-uring");

        run_server(
            shell,
            uring,
            configs_folder,
            chains_folder,
            genesis_path,
            wallets_path,
            general_path,
            secrets_path,
            contracts_path,
            additional_args,
            execution_mode,
            server_mode,
            tag,
        )
        .await?;

        Ok(())
    }

    /// Builds the server.
    pub fn build(&self, shell: &Shell) -> anyhow::Result<()> {
        let _dir_guard = shell.push_dir(&self.code_path);
        Cmd::new(cmd!(shell, "cargo build --release --bin zksync_server")).run()?;
        Ok(())
    }

    /// Returns the components as a comma-separated string.
    fn components(&self) -> Option<String> {
        self.components.as_ref().and_then(|components| {
            if components.is_empty() {
                return None;
            }
            Some(components.join(","))
        })
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_server<P>(
    shell: &Shell,
    uring: Option<&str>,
    configs_folder: P,
    chains_folder: P,
    genesis_path: P,
    wallets_path: P,
    general_path: P,
    secrets_path: P,
    contracts_path: P,
    additional_args: Vec<String>,
    execution_mode: ExecutionMode,
    server_mode: ServerMode,
    tag: Option<String>,
) -> anyhow::Result<()>
where
    P: AsRef<OsStr>,
{
    let mut cmd = match execution_mode {
        ExecutionMode::Release => Cmd::new(
            cmd!(
                shell,
                "cargo run --release --bin zksync_server {uring...} --
                    --genesis-path {genesis_path}
                    --wallets-path {wallets_path}
                    --config-path {general_path}
                    --secrets-path {secrets_path}
                    --contracts-config-path {contracts_path}
                    "
            )
            .args(additional_args)
            .env_remove("RUSTUP_TOOLCHAIN"),
        ),
        ExecutionMode::Debug => Cmd::new(
            cmd!(
                shell,
                "cargo run --bin zksync_server {uring...} --
                    --genesis-path {genesis_path}
                    --wallets-path {wallets_path}
                    --config-path {general_path}
                    --secrets-path {secrets_path}
                    --contracts-config-path {contracts_path}
                    "
            )
            .args(additional_args)
            .env_remove("RUSTUP_TOOLCHAIN"),
        ),
        ExecutionMode::Docker => {
            let tag = tag.unwrap_or(select_tag().await?.to_owned());

            Cmd::new(cmd!(
                shell,
                "docker run
                --platform linux/amd64
                --net=host
                -v {configs_folder}:/configs
                -v {chains_folder}:/chains
                matterlabs/server-v2:{tag}
                --genesis-path {genesis_path}
                --wallets-path {wallets_path}
                --config-path {general_path}
                --secrets-path {secrets_path}
                --contracts-config-path {contracts_path}
                {additional_args...}"
            ))
        }
    };

    // If we are running server in normal mode
    // we need to get the output to the console
    if let ServerMode::Normal = server_mode {
        cmd = cmd.with_force_run();
    }

    cmd.run().context("Failed to run server")
}

async fn select_tag() -> anyhow::Result<String> {
    let fetcher = GitHubTagFetcher::new(None)?;
    let gh_tags = fetcher.get_newest_core_tags(Some(5)).await?;

    let tags: Vec<String> = std::iter::once("latest".to_string())
        .chain(
            gh_tags
                .iter()
                .map(|r| r.name.trim_start_matches("core-").to_string()),
        )
        .collect();

    Ok(PromptSelect::new("Select image", tags).ask().into())
}
