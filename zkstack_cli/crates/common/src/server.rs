use std::{ffi::OsStr, path::PathBuf};

use anyhow::Context;
use xshell::{cmd, Shell};

use crate::cmd::Cmd;

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
        ExecutionMode::Release => cargo_run(
            shell,
            true,
            uring,
            genesis_path,
            wallets_path,
            general_path,
            secrets_path,
            contracts_path,
            additional_args,
        ),
        ExecutionMode::Debug => cargo_run(
            shell,
            false,
            uring,
            genesis_path,
            wallets_path,
            general_path,
            secrets_path,
            contracts_path,
            additional_args,
        ),
        ExecutionMode::Docker => {
            // safe to unwrap when ExecutionMode is Docker because we invoke fill_values_with_prompt
            let tag = tag.unwrap();

            Cmd::new(cmd!(
                shell,
                "docker run
                --platform linux/amd64
                --net=host
                -v {genesis_path}:/genesis.yaml
                -v {wallets_path}:/wallets.yaml
                -v {general_path}:/general.yaml
                -v {secrets_path}:/secrets.yaml
                -v {contracts_path}:/contracts.yaml
                matterlabs/server-v2:{tag}
                --genesis-path /genesis.yaml
                --wallets-path /wallets.yaml
                --config-path /general.yaml
                --secrets-path /secrets.yaml
                --contracts-config-path /contracts.yaml
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

#[allow(clippy::too_many_arguments)]
fn cargo_run<'a, P>(
    shell: &'a Shell,
    release: bool,
    uring: Option<&str>,
    genesis_path: P,
    wallets_path: P,
    general_path: P,
    secrets_path: P,
    contracts_path: P,
    additional_args: Vec<String>,
) -> Cmd<'a>
where
    P: AsRef<OsStr>,
{
    let compilation_mode: &str = if release { "--release" } else { "" };

    Cmd::new(
        cmd!(
            shell,
            "cargo run {compilation_mode} --bin zksync_server {uring...} --
            --genesis-path {genesis_path}
            --wallets-path {wallets_path}
            --config-path {general_path}
            --secrets-path {secrets_path}
            --contracts-config-path {contracts_path}
            "
        )
        .args(additional_args)
        .env_remove("RUSTUP_TOOLCHAIN"),
    )
}
