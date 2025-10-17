use std::{
    fs,
    io::IsTerminal as _,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use clap::Parser;
use ethers::middleware::Middleware as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::task::block_in_place;
use xshell::{cmd, Shell};

use super::script::{ForgeScript, ForgeScriptArg};
use crate::{
    cmd::{Cmd, CmdResult},
    ethereum::get_ethers_provider,
};

#[derive(Debug, Clone)]
enum ForgeRunnerMode {
    Local,
    Docker { image: String, workdir: PathBuf },
}

#[derive(Debug, Clone)]
struct DockerMounts {
    script_config: PathBuf,
    script_out: PathBuf,
    broadcast: PathBuf,
}

impl DockerMounts {
    fn new(base_path: &Path) -> anyhow::Result<Self> {
        let script_config = base_path.join("script-config");
        let script_out = base_path.join("script-out");
        let broadcast = base_path.join("broadcast");

        fs::create_dir_all(&script_config)?;
        fs::create_dir_all(&script_out)?;
        fs::create_dir_all(&broadcast)?;

        Ok(Self {
            script_config,
            script_out,
            broadcast,
        })
    }
}

/// Result of a forge script execution containing the broadcast JSON payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForgeScriptRun {
    pub script: PathBuf,
    pub broadcast_file: PathBuf,
    pub payload: Value,
}

impl ForgeScriptRun {
    pub fn transactions(&self) -> Option<&[Value]> {
        self.payload
            .get("transactions")
            .and_then(|value| value.as_array())
            .map(|array| array.as_slice())
    }
}

/// Arguments controlling how forge scripts are executed (local vs. docker, output handling, etc).
#[derive(Debug, Serialize, Deserialize, Parser, Clone)]
#[clap(next_help_heading = "Forge runner options")]
#[derive(Default)]
pub struct ForgeRunnerArgs {
    /// Use forge scripts and binaries from a Dockerized protocol image.
    /// Example: matterlabs/protocol:v0.29.0
    #[clap(long = "protocol-image", alias = "image")]
    pub docker_image: Option<String>,
    /// Append each broadcast run into the specified JSON file.
    #[clap(long = "out")]
    pub out: Option<PathBuf>,
}

pub struct ForgeRunner {
    mode: ForgeRunnerMode,
    out: Option<PathBuf>,
    runs: Vec<ForgeScriptRun>,
}

impl Default for ForgeRunner {
    fn default() -> Self {
        Self {
            mode: ForgeRunnerMode::Local,
            out: None,
            runs: Vec::new(),
        }
    }
}

impl ForgeRunner {
    pub fn new(args: ForgeRunnerArgs) -> Self {
        let mode = if let Some(image) = args.docker_image.clone() {
            ForgeRunnerMode::Docker {
                image,
                workdir: PathBuf::from("/contracts/l1-contracts"),
            }
        } else {
            ForgeRunnerMode::Local
        };

        Self {
            mode,
            out: args.out,
            runs: Vec::new(),
        }
    }

    pub fn run(&mut self, shell: &Shell, mut script: ForgeScript) -> anyhow::Result<()> {
        if script.needs_bridgehub_skip() {
            let skip_path: String = String::from("contracts/bridgehub/*");
            script.args.add_arg(ForgeScriptArg::Skip { skip_path });
        }

        let use_docker = matches!(self.mode, ForgeRunnerMode::Docker { .. });

        let mut args_no_resume = script.args.clone();
        let args_no_resume = args_no_resume.build_for_runner(use_docker);

        let command_result = if script.args.resume {
            let mut args_with_resume = args_no_resume.clone();
            args_with_resume.push(ForgeScriptArg::Resume.to_string());
            let res = self.execute(shell, &script, &args_with_resume, true)?;
            if res.resume_not_successful_because_has_not_began() {
                self.execute(shell, &script, &args_no_resume, false)?
            } else {
                res
            }
        } else {
            self.execute(shell, &script, &args_no_resume, false)?
        };

        if command_result.proposal_error() {
            return Ok(());
        }

        if command_result.is_ok() {
            let run = self.collect_run(&script)?;
            if let Some(ref payload) = run {
                self.record_run(payload)?;
            }
        }
        Ok(command_result?)
    }

    pub fn runs(&self) -> &[ForgeScriptRun] {
        &self.runs
    }

    pub fn into_runs(self) -> Vec<ForgeScriptRun> {
        self.runs
    }

    fn execute(
        &self,
        shell: &Shell,
        script: &ForgeScript,
        args: &[String],
        for_resume: bool,
    ) -> anyhow::Result<CmdResult<()>> {
        match &self.mode {
            ForgeRunnerMode::Local => {
                let script_path = script.script_name().as_os_str();
                let _dir_guard = shell.push_dir(script.base_path());
                let mut cmd =
                    Cmd::new(cmd!(shell, "forge script {script_path} --legacy {args...}"));
                if for_resume {
                    cmd = cmd.with_piped_std_err();
                }
                Ok(cmd.run())
            }
            ForgeRunnerMode::Docker { image, workdir } => {
                if for_resume {
                    bail!("Resume is not supported for Dockerized protocol images");
                }
                let mounts = DockerMounts::new(script.base_path())?;
                let mut docker_args: Vec<String> = vec![
                    "--rm".to_string(),
                    "--platform".to_string(),
                    "linux/amd64".to_string(),
                    "--add-host=host.docker.internal:host-gateway".to_string(),
                    format!("--workdir={}", workdir.display()),
                    format!(
                        "-v={}:{}",
                        mounts.script_config.display(),
                        workdir.join("script-config").display()
                    ),
                    format!(
                        "-v={}:{}",
                        mounts.script_out.display(),
                        workdir.join("script-out").display()
                    ),
                    format!(
                        "-v={}:{}",
                        mounts.broadcast.display(),
                        workdir.join("broadcast").display()
                    ),
                ];

                if std::io::stdin().is_terminal() {
                    docker_args.push("-i".to_string());
                }
                if std::io::stdout().is_terminal() {
                    docker_args.push("-t".to_string());
                }

                let script_path = script.script_name().as_os_str();
                let cmd = Cmd::new(cmd!(
                    shell,
                    "docker run {docker_args...} {image} forge script {script_path} --legacy {args...}"
                ))
                .with_force_run();
                Ok(cmd.run())
            }
        }
    }

    fn collect_run(&self, script: &ForgeScript) -> anyhow::Result<Option<ForgeScriptRun>> {
        let Some(broadcast_file) = self.locate_latest_broadcast(script)? else {
            println!("no broadcast file found");
            return Ok(None);
        };
        println!("broadcast file: {}", broadcast_file.display());
        let payload = read_json(&broadcast_file)?;
        Ok(Some(ForgeScriptRun {
            script: script.script_name().to_path_buf(),
            broadcast_file,
            payload,
        }))
    }

    fn locate_latest_broadcast(&self, script: &ForgeScript) -> anyhow::Result<Option<PathBuf>> {
        let root = script.base_path().join("broadcast");
        if !root.exists() {
            return Ok(None);
        }
        let Some(script_name) = script.script_name().file_name() else {
            return Ok(None);
        };
        let rpc_url = script
            .rpc_url()
            .context("failed to get rpc url to query chain id")?;
        let l1_chain_id = query_chain_id_sync(&rpc_url)?;
        let script_dir = root.join(script_name).join(l1_chain_id.to_string());
        if !script_dir.exists() {
            return Ok(None);
        }
        let run_latest_filename = derive_latest_filename(script.sig());
        let run_latest_path = script_dir.join(run_latest_filename);
        if run_latest_path.exists() {
            Ok(Some(run_latest_path))
        } else {
            Ok(None)
        }
    }

    fn record_run(&mut self, run: &ForgeScriptRun) -> anyhow::Result<()> {
        if let Some(out_path) = &self.out {
            append_to_out(out_path, &run.payload)?;
        }
        self.runs.push(run.clone());
        Ok(())
    }
}

// Trait for handling forge errors. Required for implementing method for CmdResult
pub(crate) trait ForgeErrorHandler {
    // Resume doesn't work if the forge script has never been started on this chain before.
    // So we want to catch it and try again without resume arg if it's the case
    fn resume_not_successful_because_has_not_began(&self) -> bool;
    // Catch the error if upgrade tx has already been processed. We do execute much of
    // txs using upgrade mechanism and if this particular upgrade has already been processed we could assume
    // it as a success
    fn proposal_error(&self) -> bool;
}

impl ForgeErrorHandler for CmdResult<()> {
    fn resume_not_successful_because_has_not_began(&self) -> bool {
        let text = "Deployment not found for chain";
        check_error(self, text)
    }

    fn proposal_error(&self) -> bool {
        let text = "revert: Operation with this proposal id already exists";
        check_error(self, text)
    }
}

fn check_error(cmd_result: &CmdResult<()>, error_text: &str) -> bool {
    if let Err(cmd_error) = &cmd_result {
        if let Some(stderr) = &cmd_error.stderr {
            return stderr.contains(error_text);
        }
    }
    false
}

/// Derive the latest.json filename from an optional --sig value:
/// 1) no sig          -> "run-latest.json"
/// 2) hex sig         -> "<first8hex>-latest.json"   (strip 0x, case-insensitive)
/// 3) non-hex sig     -> "<sig>-latest.json"
fn derive_latest_filename(sig: Option<String>) -> String {
    fn is_hex_like(s: &str) -> bool {
        let s = s.strip_prefix("0x").unwrap_or(s);
        !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
    }

    match sig {
        None => "run-latest.json".to_string(),
        Some(raw) => {
            let trimmed = raw.trim();
            if is_hex_like(trimmed) {
                let no_prefix = trimmed.strip_prefix("0x").unwrap_or(trimmed);
                // lowercasing is optional, but helps consistency
                let lower = no_prefix.to_ascii_lowercase();
                let prefix8 = &lower[..lower.len().min(8)];
                format!("{prefix8}-latest.json")
            } else {
                format!("{trimmed}-latest.json")
            }
        }
    }
}

fn read_json(path: &Path) -> anyhow::Result<Value> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read forge broadcast file {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| {
        format!(
            "failed to parse forge broadcast file {} as JSON",
            path.display()
        )
    })
}

fn append_to_out(path: &Path, payload: &Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut runs: Vec<Value> = if path.exists() {
        let existing = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if existing.trim().is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&existing)
                .with_context(|| format!("failed to parse existing JSON in {}", path.display()))?
        }
    } else {
        Vec::new()
    };

    runs.push(payload.clone());
    let serialized = serde_json::to_string_pretty(&runs)?;
    fs::write(path, serialized)
        .with_context(|| format!("failed to write JSON output to {}", path.display()))?;
    Ok(())
}

fn query_chain_id_sync(rpc_url: &str) -> anyhow::Result<u64> {
    let provider = get_ethers_provider(rpc_url)?;
    let fut = provider.get_chainid();
    let id = if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(fut))?
    } else {
        tokio::runtime::Runtime::new()
            .context("failed to create Tokio runtime")?
            .block_on(fut)?
    };

    Ok(id.as_u64())
}
