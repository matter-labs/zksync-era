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
#[derive(Clone, Debug, Default)]
pub enum ExecutionMode {
    #[default]
    Release,
    Debug,
    Docker {
        tag: String,
    },
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
        ports: Vec<u16>,
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
            ports,
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
    ports: Vec<u16>,
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
        ExecutionMode::Docker { tag } => docker_run(
            shell,
            genesis_path,
            wallets_path,
            general_path,
            secrets_path,
            contracts_path,
            additional_args,
            tag,
            ports,
        ),
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

#[allow(clippy::too_many_arguments)]
fn docker_run<P>(
    shell: &Shell,
    genesis_path: P,
    wallets_path: P,
    general_path: P,
    secrets_path: P,
    contracts_path: P,
    additional_args: Vec<String>,
    tag: String,
    ports: Vec<u16>,
) -> Cmd<'_>
where
    P: AsRef<OsStr>,
{
    let genesis_path = genesis_path.as_ref().to_string_lossy();
    let wallets_path = wallets_path.as_ref().to_string_lossy();
    let general_path = general_path.as_ref().to_string_lossy();
    let secrets_path = secrets_path.as_ref().to_string_lossy();
    let contracts_path = contracts_path.as_ref().to_string_lossy();

    // do not expose postgres and reth ports
    let ports = ports
        .into_iter()
        .filter(|p| *p != 5432 && *p != 8545)
        .collect::<Vec<_>>();

    let mut cmd = cmd!(shell, "docker run")
        .arg("--platform")
        .arg("linux/amd64")
        .arg("-v")
        .arg(format!("{genesis_path}:/config/genesis.yaml"))
        .arg("-v")
        .arg(format!("{wallets_path}:/config/wallets.yaml"))
        .arg("-v")
        .arg(format!("{general_path}:/config/general.yaml"))
        .arg("-v")
        .arg(format!("{secrets_path}:/config/secrets.yaml"))
        .arg("-v")
        .arg(format!("{contracts_path}:/config/contracts.yaml"));

    for p in ports {
        cmd = cmd.arg("-p").arg(format!("{p}:{p}"));
    }

    cmd = cmd
        .arg(format!("matterlabs/server-v2:{tag}"))
        .arg("--genesis-path")
        .arg("/config/genesis.yaml")
        .arg("--wallets-path")
        .arg("/config/wallets.yaml")
        .arg("--config-path")
        .arg("/config/general.yaml")
        .arg("--secrets-path")
        .arg("/config/secrets.yaml")
        .arg("--contracts-config-path")
        .arg("/config/contracts.yaml")
        .args(additional_args);

    Cmd::new(cmd)
}
