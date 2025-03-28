use std::{ffi::OsStr, path::PathBuf};

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
    pub fn run<P>(
        &self,
        shell: &Shell,
        server_mode: ServerMode,
        genesis_path: P,
        wallets_path: P,
        general_path: P,
        secrets_path: P,
        contracts_path: P,
        gateway_contracts_config_path: Option<P>,
        mut additional_args: Vec<String>,
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

        let (gateway_config_param, gateway_config_path) =
            if let Some(gateway_contracts_config_path) = gateway_contracts_config_path {
                (
                    Some("--gateway-contracts-config-path"),
                    Some(gateway_contracts_config_path),
                )
            } else {
                (None, None)
            };

        let mut cmd = Cmd::new(
            cmd!(
                shell,
                "cargo run --manifest-path ./core/Cargo.toml --release --bin zksync_server {uring...} --
                --genesis-path {genesis_path}
                --wallets-path {wallets_path}
                --config-path {general_path}
                --secrets-path {secrets_path}
                --contracts-config-path {contracts_path}
                {gateway_config_param...} {gateway_config_path...}
                "
            )
            .args(additional_args)
            .env_remove("RUSTUP_TOOLCHAIN"),
        );

        // If we are running server in normal mode
        // we need to get the output to the console
        if let ServerMode::Normal = server_mode {
            cmd = cmd.with_force_run();
        }

        cmd.run()?;

        Ok(())
    }

    /// Builds the server.
    pub fn build(&self, shell: &Shell) -> anyhow::Result<()> {
        let _dir_guard = shell.push_dir(self.code_path.join("core"));
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
