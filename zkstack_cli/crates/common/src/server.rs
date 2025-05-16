use std::{ffi::OsStr, path::PathBuf};

use xshell::{cmd, Shell};

use crate::cmd::Cmd;

/// Default command to run the server; will use `cargo` to build it.
const DEFAULT_SERVER_COMMAND: &str =
    "cargo run --manifest-path ./core/Cargo.toml --release --bin zksync_server";

/// Allows to perform server operations.
#[derive(Debug)]
pub struct Server {
    /// Command to run the server, could be a path to the binary.
    /// If not set, the server will be built using cargo.
    server_command: Option<String>,
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
    pub fn new(
        server_command: Option<String>,
        components: Option<Vec<String>>,
        code_path: PathBuf,
        uring: bool,
    ) -> Self {
        Self {
            server_command,
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
        additional_args: Vec<String>,
    ) -> anyhow::Result<()>
    where
        P: AsRef<OsStr>,
    {
        let _dir_guard = shell.push_dir(&self.code_path);

        let mut inserted_args = vec![];
        if let Some(components) = self.components() {
            inserted_args.push(format!("--components={}", components))
        }
        if let ServerMode::Genesis = server_mode {
            inserted_args.push("--genesis".to_string());
        }

        let uring = self.uring.then_some("--features=rocksdb/io-uring");

        let server_command = match &self.server_command {
            Some(command) => {
                // We assume that if the user provides a custom server command,
                // they can include any feature flags they need themselves.
                if uring.is_some() {
                    return Err(anyhow::anyhow!(
                        "Cannot use uring with a custom server command"
                    ));
                }
                command.clone()
            }
            None => {
                let uring = uring.unwrap_or_default();
                if uring.is_empty() {
                    format!("{DEFAULT_SERVER_COMMAND} --")
                } else {
                    format!("{DEFAULT_SERVER_COMMAND} {uring} --")
                }
            }
        };
        let mut server_command = server_command.split_ascii_whitespace().collect::<Vec<_>>();

        let (command, args) = server_command.split_at_mut(1);

        let mut cmd = Cmd::new(
            shell
                .cmd(command[0])
                .args(args)
                .arg("--genesis-path")
                .arg(genesis_path)
                .arg("--config-path")
                .arg(general_path)
                .arg("--wallets-path")
                .arg(wallets_path)
                .arg("--secrets-path")
                .arg(secrets_path)
                .arg("--contracts-config-path")
                .arg(contracts_path)
                .args(inserted_args)
                .args(additional_args) // Need to insert the additional args at the end, since they may include positional ones
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
