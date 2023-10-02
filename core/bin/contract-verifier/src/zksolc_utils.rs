use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;

use crate::error::ContractVerifierError;

#[derive(Debug)]
pub enum ZkSolcInput {
    StandardJson(StandardJson),
    YulSingleFile(String),
}

#[derive(Debug)]
pub enum ZkSolcOutput {
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

#[derive(Debug, Serialize, Deserialize)]
pub enum MetadataHash {
    /// Do not include bytecode hash.
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    /// The bytecode hash mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytecode_hash: Option<MetadataHash>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The linker library addresses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub libraries: Option<HashMap<String, HashMap<String, String>>>,
    /// The output selection filters.
    pub output_selection: Option<serde_json::Value>,
    /// The optimizer settings.
    #[serde(default)]
    pub optimizer: Optimizer,
    /// The metadata settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Flag for system compilation mode.
    #[serde(default)]
    pub is_system: bool,
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

impl Optimizer {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            mode: None,
        }
    }
}

pub struct ZkSolc {
    zksolc_path: PathBuf,
    solc_path: PathBuf,
}

impl ZkSolc {
    pub fn new(zksolc_path: impl Into<PathBuf>, solc_path: impl Into<PathBuf>) -> Self {
        ZkSolc {
            zksolc_path: zksolc_path.into(),
            solc_path: solc_path.into(),
        }
    }

    pub async fn async_compile(
        &self,
        input: ZkSolcInput,
    ) -> Result<ZkSolcOutput, ContractVerifierError> {
        use tokio::io::AsyncWriteExt;
        let mut command = tokio::process::Command::new(&self.zksolc_path);
        if let ZkSolcInput::StandardJson(input) = &input {
            if input.settings.is_system {
                command.arg("--system-mode");
            }
        }
        command
            .arg("--solc")
            .arg(self.solc_path.to_str().unwrap())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        match input {
            ZkSolcInput::StandardJson(input) => {
                let mut child = command
                    .arg("--standard-json")
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|_err| ContractVerifierError::InternalError)?;
                let stdin = child.stdin.as_mut().unwrap();
                let content = serde_json::to_vec(&input).unwrap();
                stdin
                    .write_all(&content)
                    .await
                    .map_err(|_err| ContractVerifierError::InternalError)?;
                stdin
                    .flush()
                    .await
                    .map_err(|_err| ContractVerifierError::InternalError)?;

                let output = child
                    .wait_with_output()
                    .await
                    .map_err(|_err| ContractVerifierError::InternalError)?;
                if output.status.success() {
                    Ok(ZkSolcOutput::StandardJson(
                        serde_json::from_slice(&output.stdout)
                            .expect("Compiler output must be valid JSON"),
                    ))
                } else {
                    Err(ContractVerifierError::CompilerError(
                        "zksolc".to_string(),
                        String::from_utf8_lossy(&output.stderr).to_string(),
                    ))
                }
            }
            ZkSolcInput::YulSingleFile(content) => {
                let mut file = tempfile::Builder::new()
                    .prefix("input")
                    .suffix(".yul")
                    .rand_bytes(0)
                    .tempfile()
                    .map_err(|_err| ContractVerifierError::InternalError)?;
                file.write_all(content.as_bytes())
                    .map_err(|_err| ContractVerifierError::InternalError)?;
                let child = command
                    .arg(file.path().to_str().unwrap())
                    .arg("--optimization")
                    .arg("3")
                    .arg("--yul")
                    .arg("--bin")
                    .spawn()
                    .map_err(|_err| ContractVerifierError::InternalError)?;
                let output = child
                    .wait_with_output()
                    .await
                    .map_err(|_err| ContractVerifierError::InternalError)?;
                if output.status.success() {
                    Ok(ZkSolcOutput::YulSingleFile(
                        String::from_utf8(output.stdout).expect("Couldn't parse string"),
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
}
