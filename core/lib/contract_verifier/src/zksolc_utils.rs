use std::{collections::HashMap, io::Write, path::PathBuf, process::Stdio};

use anyhow::Context as _;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::ContractVerifierError;

#[derive(Debug)]
pub enum ZkSolcInput {
    StandardJson(StandardJson),
    YulSingleFile {
        source_code: String,
        is_system: bool,
    },
}

#[derive(Debug)]
pub enum ZkSolcOutput {
    // FIXME: incorrect abstraction boundary
    StandardJson(serde_json::Value),
    YulSingleFile(String),
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

pub struct ZkSolc {
    zksolc_path: PathBuf,
    solc_path: PathBuf,
    zksolc_version: String,
}

impl ZkSolc {
    pub fn new(
        zksolc_path: impl Into<PathBuf>,
        solc_path: impl Into<PathBuf>,
        zksolc_version: String,
    ) -> Self {
        ZkSolc {
            zksolc_path: zksolc_path.into(),
            solc_path: solc_path.into(),
            zksolc_version,
        }
    }

    pub async fn async_compile(
        &self,
        input: ZkSolcInput,
    ) -> Result<ZkSolcOutput, ContractVerifierError> {
        use tokio::io::AsyncWriteExt;
        let mut command = tokio::process::Command::new(&self.zksolc_path);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        match &input {
            ZkSolcInput::StandardJson(input) => {
                if !self.is_post_1_5_0() {
                    if input.settings.is_system {
                        command.arg("--system-mode");
                    }
                    if input.settings.force_evmla {
                        command.arg("--force-evmla");
                    }
                }

                command.arg("--solc").arg(self.solc_path.to_str().unwrap());
            }
            ZkSolcInput::YulSingleFile { is_system, .. } => {
                if self.is_post_1_5_0() {
                    if *is_system {
                        command.arg("--enable-eravm-extensions");
                    } else {
                        command.arg("--solc").arg(self.solc_path.to_str().unwrap());
                    }
                } else {
                    if *is_system {
                        command.arg("--system-mode");
                    }
                    command.arg("--solc").arg(self.solc_path.to_str().unwrap());
                }
            }
        }
        match input {
            ZkSolcInput::StandardJson(input) => {
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
                    Ok(ZkSolcOutput::StandardJson(
                        serde_json::from_slice(&output.stdout)
                            .context("zksolc output is not valid JSON")?,
                    ))
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
                    Ok(ZkSolcOutput::YulSingleFile(
                        String::from_utf8(output.stdout).context("zksolc output is not UTF-8")?,
                    ))
                } else {
                    Err(ContractVerifierError::CompilerError(
                        "zksolc".to_string(),
                        String::from_utf8_lossy(&output.stderr).to_string(),
                    ))
                }
            }
        }
    }

    pub fn is_post_1_5_0(&self) -> bool {
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

#[cfg(test)]
mod tests {
    use crate::zksolc_utils::ZkSolc;

    #[test]
    fn check_is_post_1_5_0() {
        // Special case.
        let mut zksolc = ZkSolc::new(".", ".", "vm-1.5.0-a167aa3".to_string());
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
