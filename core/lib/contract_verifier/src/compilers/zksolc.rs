use std::{collections::HashMap, io::Write, process::Stdio};

use anyhow::Context as _;
use regex::Regex;
use semver::Version;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification::api::{
    CompilationArtifacts, SourceCodeData, VerificationIncomingRequest,
};

use super::{parse_standard_json_output, process_contract_name, Source};
use crate::{
    error::ContractVerifierError,
    resolver::{Compiler, CompilerPaths},
};

#[derive(Debug)]
pub(crate) enum ZkSolcInput {
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
pub(crate) struct StandardJson {
    /// The input language.
    pub language: String,
    /// The input source code files hashmap.
    pub sources: HashMap<String, Source>,
    /// The compiler settings.
    pub settings: Settings,
}

/// Compiler settings.
/// There are fields like `output_selection`, `is_system`, `force_evmla` which are accessed by contract verifier explicitly.
/// Other fields are accumulated in `other`, this way every field that was in the original request will be passed to a compiler.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Settings {
    /// The output selection filters.
    pub output_selection: Option<serde_json::Value>,
    /// Flag for system compilation mode.
    #[serde(default)]
    pub is_system: bool,
    /// Flag to force `evmla` IR.
    #[serde(default)]
    pub force_evmla: bool,
    /// Other settings (only filled when parsing `StandardJson` input from the request).
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Optimizer {
    /// Whether the optimizer is enabled.
    pub enabled: bool,
    /// The optimization mode string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<char>,
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

    pub fn build_input(
        req: VerificationIncomingRequest,
    ) -> Result<ZkSolcInput, ContractVerifierError> {
        let (file_name, contract_name) = process_contract_name(&req.contract_name, "sol");
        let default_output_selection = serde_json::json!({
            "*": {
                "*": [ "abi" ],
                 "": [ "abi" ]
            }
        });

        match req.source_code_data {
            SourceCodeData::SolSingleFile(source_code) => {
                let source = Source {
                    content: source_code,
                };
                let sources = HashMap::from([(file_name.clone(), source)]);
                let settings = Settings {
                    output_selection: Some(default_output_selection),
                    is_system: req.is_system,
                    force_evmla: req.force_evmla,
                    other: serde_json::json!({
                        "optimizer": Optimizer {
                            enabled: req.optimization_used,
                            mode: req.optimizer_mode.and_then(|s| s.chars().next()),
                        },
                    }),
                };

                Ok(ZkSolcInput::StandardJson {
                    input: StandardJson {
                        language: "Solidity".to_string(),
                        sources,
                        settings,
                    },
                    contract_name,
                    file_name,
                })
            }
            SourceCodeData::StandardJsonInput(map) => {
                let mut compiler_input: StandardJson =
                    serde_json::from_value(serde_json::Value::Object(map))
                        .map_err(|_| ContractVerifierError::FailedToDeserializeInput)?;
                // Set default output selection even if it is different in request.
                compiler_input.settings.output_selection = Some(default_output_selection);
                Ok(ZkSolcInput::StandardJson {
                    input: compiler_input,
                    contract_name,
                    file_name,
                })
            }
            SourceCodeData::YulSingleFile(source_code) => Ok(ZkSolcInput::YulSingleFile {
                source_code,
                is_system: req.is_system,
            }),
            other => unreachable!("Unexpected `SourceCodeData` variant: {other:?}"),
        }
    }

    fn parse_single_file_yul_output(
        output: &str,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let cap = if output.contains("Binary:\n") {
            // Format of the new output
            // ======= /tmp/input.yul:Empty =======
            // Binary:
            // 00000001002 <..>
            let re = Regex::new(r"Binary:\n([\da-f]+)").unwrap();
            re.captures(output)
                .with_context(|| format!("Yul output doesn't match regex. Output: {output}"))?
        } else {
            // Old compiler versions
            let re_old = Regex::new(r"Contract `.*` bytecode: 0x([\da-f]+)").unwrap();
            re_old
                .captures(output)
                .with_context(|| format!("Yul output doesn't match regex. Output: {output}"))?
        };
        let bytecode_str = cap.get(1).context("no matches in Yul output")?.as_str();
        let bytecode = hex::decode(bytecode_str).context("invalid Yul output bytecode")?;

        Ok(CompilationArtifacts {
            bytecode,
            deployed_bytecode: None,
            abi: serde_json::Value::Array(Vec::new()),
            immutable_refs: Default::default(),
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
                    parse_standard_json_output(&output, contract_name, file_name, false)
                } else {
                    Err(ContractVerifierError::CompilerError(
                        "zksolc",
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

                // TODO: `zksolc` support standard JSON for `yul` since 1.5.0, so we don't have
                // to parse `--bin` output.
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
                        "zksolc",
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

    use super::*;

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
