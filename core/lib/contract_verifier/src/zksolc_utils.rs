use std::{collections::HashMap, io::Write, process::Stdio};

use anyhow::Context as _;
use regex::Regex;
use semver::Version;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification_api::CompilationArtifacts;

use crate::{
    error::ContractVerifierError,
    resolver::{Compiler, CompilerPaths},
};

#[derive(Debug)]
pub enum ZkSolcInput {
    StandardJson {
        input: StandardJson,
        contract_name: String,
        file_name: String,
    },
    YulSingleFile {
        source_code: String,
        is_system: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandardJson {
    /// The input language.
    pub language: String,
    /// The input source code files hashmap.
    pub sources: HashMap<String, Source>,
    /// The compiler settings.
    pub settings: Settings,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// The source code file content.
    pub content: String,
}

/// Compiler settings.
/// There are fields like `output_selection`, `is_system`, `force_evmla` which are accessed by contract verifier explicitly.
/// Other fields are accumulated in `other`, this way every field that was in the original request will be passed to a compiler.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The output selection filters.
    pub output_selection: Option<serde_json::Value>,
    /// Flag for system compilation mode.
    #[serde(default)]
    pub is_system: bool,
    /// Flag to force `evmla` IR.
    #[serde(default)]
    pub force_evmla: bool,
    /// Other fields.
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Optimizer {
    /// Whether the optimizer is enabled.
    pub enabled: bool,
    /// The optimization mode string.
    pub mode: Option<char>,
}

impl Default for Optimizer {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ZkSolc {
    paths: CompilerPaths,
    zksolc_version: String,
}

impl ZkSolc {
    pub fn new(paths: CompilerPaths, zksolc_version: String) -> Self {
        ZkSolc {
            paths,
            zksolc_version,
        }
    }

    fn parse_standard_json_output(
        output: &serde_json::Value,
        contract_name: String,
        file_name: String,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        if let Some(errors) = output.get("errors") {
            let errors = errors.as_array().unwrap().clone();
            if errors
                .iter()
                .any(|err| err["severity"].as_str().unwrap() == "error")
            {
                let error_messages = errors
                    .into_iter()
                    .map(|err| err["formattedMessage"].clone())
                    .collect();
                return Err(ContractVerifierError::CompilationError(
                    serde_json::Value::Array(error_messages),
                ));
            }
        }

        let contracts = output["contracts"]
            .get(&file_name)
            .ok_or(ContractVerifierError::MissingSource(file_name))?;
        let Some(contract) = contracts.get(&contract_name) else {
            return Err(ContractVerifierError::MissingContract(contract_name));
        };
        let bytecode_str = contract["evm"]["bytecode"]["object"]
            .as_str()
            .ok_or(ContractVerifierError::AbstractContract(contract_name))?;
        let bytecode = hex::decode(bytecode_str).unwrap();
        let abi = contract["abi"].clone();
        if !abi.is_array() {
            let err = anyhow::anyhow!(
                "zksolc returned unexpected value for ABI: {}",
                serde_json::to_string_pretty(&abi).unwrap()
            );
            return Err(err.into());
        }

        Ok(CompilationArtifacts { bytecode, abi })
    }

    fn parse_single_file_yul_output(
        output: &str,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let re = Regex::new(r"Contract `.*` bytecode: 0x([\da-f]+)").unwrap();
        let cap = re
            .captures(output)
            .context("Yul output doesn't match regex")?;
        let bytecode_str = cap.get(1).context("no matches in Yul output")?.as_str();
        let bytecode = hex::decode(bytecode_str).context("invalid Yul output bytecode")?;
        Ok(CompilationArtifacts {
            bytecode,
            abi: serde_json::Value::Array(Vec::new()),
        })
    }

    fn is_post_1_5_0(&self) -> bool {
        // Special case
        if &self.zksolc_version == "vm-1.5.0-a167aa3" {
            false
        } else if let Some(version) = self.zksolc_version.strip_prefix("v") {
            if let Ok(semver) = Version::parse(version) {
                let target = Version::new(1, 5, 0);
                semver >= target
            } else {
                true
            }
        } else {
            true
        }
    }
}

#[async_trait]
impl Compiler<ZkSolcInput> for ZkSolc {
    async fn compile(
        self: Box<Self>,
        input: ZkSolcInput,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let mut command = tokio::process::Command::new(&self.paths.zk);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        match &input {
            ZkSolcInput::StandardJson { input, .. } => {
                if !self.is_post_1_5_0() {
                    if input.settings.is_system {
                        command.arg("--system-mode");
                    }
                    if input.settings.force_evmla {
                        command.arg("--force-evmla");
                    }
                }

                command.arg("--solc").arg(&self.paths.base);
            }
            ZkSolcInput::YulSingleFile { is_system, .. } => {
                if self.is_post_1_5_0() {
                    if *is_system {
                        command.arg("--enable-eravm-extensions");
                    } else {
                        command.arg("--solc").arg(&self.paths.base);
                    }
                } else {
                    if *is_system {
                        command.arg("--system-mode");
                    }
                    command.arg("--solc").arg(&self.paths.base);
                }
            }
        }
        match input {
            ZkSolcInput::StandardJson {
                input,
                contract_name,
                file_name,
            } => {
                let mut child = command
                    .arg("--standard-json")
                    .stdin(Stdio::piped())
                    .spawn()
                    .context("failed spawning zksolc")?;
                let stdin = child.stdin.as_mut().unwrap();
                let content = serde_json::to_vec(&input)
                    .context("cannot encode standard JSON input for zksolc")?;
                stdin
                    .write_all(&content)
                    .await
                    .context("failed writing standard JSON to zksolc stdin")?;
                stdin
                    .flush()
                    .await
                    .context("failed flushing standard JSON to zksolc")?;

                let output = child.wait_with_output().await.context("zksolc failed")?;
                if output.status.success() {
                    let output = serde_json::from_slice(&output.stdout)
                        .context("zksolc output is not valid JSON")?;
                    Self::parse_standard_json_output(&output, contract_name, file_name)
                } else {
                    Err(ContractVerifierError::CompilerError(
                        "zksolc".to_string(),
                        String::from_utf8_lossy(&output.stderr).to_string(),
                    ))
                }
            }
            ZkSolcInput::YulSingleFile { source_code, .. } => {
                let mut file = tempfile::Builder::new()
                    .prefix("input")
                    .suffix(".yul")
                    .rand_bytes(0)
                    .tempfile()
                    .context("cannot create temporary Yul file")?;
                file.write_all(source_code.as_bytes())
                    .context("failed writing Yul file")?;
                let child = command
                    .arg(file.path().to_str().unwrap())
                    .arg("--optimization")
                    .arg("3")
                    .arg("--yul")
                    .arg("--bin")
                    .spawn()
                    .context("failed spawning zksolc")?;
                let output = child.wait_with_output().await.context("zksolc failed")?;
                if output.status.success() {
                    let output =
                        String::from_utf8(output.stdout).context("zksolc output is not UTF-8")?;
                    Self::parse_single_file_yul_output(&output)
                } else {
                    Err(ContractVerifierError::CompilerError(
                        "zksolc".to_string(),
                        String::from_utf8_lossy(&output.stderr).to_string(),
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{resolver::CompilerPaths, zksolc_utils::ZkSolc};

    #[test]
    fn check_is_post_1_5_0() {
        // Special case.
        let compiler_paths = CompilerPaths {
            base: PathBuf::default(),
            zk: PathBuf::default(),
        };
        let mut zksolc = ZkSolc::new(compiler_paths, "vm-1.5.0-a167aa3".to_string());
        assert!(!zksolc.is_post_1_5_0(), "vm-1.5.0-a167aa3");

        zksolc.zksolc_version = "v1.5.0".to_string();
        assert!(zksolc.is_post_1_5_0(), "v1.5.0");

        zksolc.zksolc_version = "v1.5.1".to_string();
        assert!(zksolc.is_post_1_5_0(), "v1.5.1");

        zksolc.zksolc_version = "v1.10.1".to_string();
        assert!(zksolc.is_post_1_5_0(), "v1.10.1");

        zksolc.zksolc_version = "v2.0.0".to_string();
        assert!(zksolc.is_post_1_5_0(), "v2.0.0");

        zksolc.zksolc_version = "v1.4.15".to_string();
        assert!(!zksolc.is_post_1_5_0(), "v1.4.15");

        zksolc.zksolc_version = "v1.3.21".to_string();
        assert!(!zksolc.is_post_1_5_0(), "v1.3.21");

        zksolc.zksolc_version = "v0.5.1".to_string();
        assert!(!zksolc.is_post_1_5_0(), "v0.5.1");
    }
}
