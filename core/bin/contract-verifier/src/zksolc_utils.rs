use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;

use crate::error::ContractVerifierError;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompilerInput {
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
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The linker library addresses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub libraries: Option<HashMap<String, HashMap<String, String>>>,
    /// The output selection filters.
    pub output_selection: Option<serde_json::Value>,
    /// The optimizer settings.
    pub optimizer: Optimizer,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Optimizer {
    /// Whether the optimizer is enabled.
    pub enabled: bool,
}

impl Optimizer {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
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
        input: &CompilerInput,
        is_system_flag: bool,
    ) -> Result<serde_json::Value, ContractVerifierError> {
        use tokio::io::AsyncWriteExt;
        let content = serde_json::to_vec(input).unwrap();
        let mut command = tokio::process::Command::new(&self.zksolc_path);
        if is_system_flag {
            command.arg("--system-mode");
        }
        let mut child = command
            .arg("--standard-json")
            .arg("--solc")
            .arg(self.solc_path.to_str().unwrap())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|_err| ContractVerifierError::InternalError)?;
        let stdin = child.stdin.as_mut().unwrap();
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
            Ok(serde_json::from_slice(&output.stdout).expect("Compiler output must be valid JSON"))
        } else {
            Err(ContractVerifierError::ZkSolcError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }
}
