use std::{collections::HashMap, path::PathBuf, process::Stdio};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification_api::{
    CompilationArtifacts, SourceCodeData, VerificationIncomingRequest,
};

use super::{parse_standard_json_output, Source};
use crate::{error::ContractVerifierError, resolver::Compiler};

// Here and below, fields are public for testing purposes.
#[derive(Debug)]
pub(crate) struct SolcInput {
    pub standard_json: StandardJson,
    pub contract_name: String,
    pub file_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StandardJson {
    pub language: String,
    pub sources: HashMap<String, Source>,
    settings: Settings,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    /// The output selection filters.
    output_selection: Option<serde_json::Value>,
    /// Other settings (only filled when parsing `StandardJson` input from the request).
    #[serde(flatten)]
    other: serde_json::Value,
}

#[derive(Debug)]
pub(crate) struct Solc {
    path: PathBuf,
}

impl Solc {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn build_input(
        req: VerificationIncomingRequest,
    ) -> Result<SolcInput, ContractVerifierError> {
        // Users may provide either just contract name or
        // source file name and contract name joined with ":".
        let (file_name, contract_name) =
            if let Some((file_name, contract_name)) = req.contract_name.rsplit_once(':') {
                (file_name.to_string(), contract_name.to_string())
            } else {
                (
                    format!("{}.sol", req.contract_name),
                    req.contract_name.clone(),
                )
            };
        let default_output_selection = serde_json::json!({
            "*": {
                "*": [ "abi", "evm.bytecode", "evm.deployedBytecode" ],
                 "": [ "abi", "evm.bytecode", "evm.deployedBytecode" ],
            }
        });

        let standard_json = match req.source_code_data {
            SourceCodeData::SolSingleFile(source_code) => {
                let source = Source {
                    content: source_code,
                };
                let sources = HashMap::from([(file_name.clone(), source)]);
                let settings = Settings {
                    output_selection: Some(default_output_selection),
                    other: serde_json::json!({
                        "optimizer": {
                            "enabled": req.optimization_used,
                        },
                    }),
                };

                StandardJson {
                    language: "Solidity".to_owned(),
                    sources,
                    settings,
                }
            }
            SourceCodeData::StandardJsonInput(map) => {
                let mut compiler_input: StandardJson =
                    serde_json::from_value(serde_json::Value::Object(map))
                        .map_err(|_| ContractVerifierError::FailedToDeserializeInput)?;
                // Set default output selection even if it is different in request.
                compiler_input.settings.output_selection = Some(default_output_selection);
                compiler_input
            }
            SourceCodeData::YulSingleFile(source_code) => {
                let source = Source {
                    content: source_code,
                };
                let sources = HashMap::from([(file_name.clone(), source)]);
                let settings = Settings {
                    output_selection: Some(default_output_selection),
                    other: serde_json::json!({
                        "optimizer": {
                            "enabled": req.optimization_used,
                        },
                    }),
                };
                StandardJson {
                    language: "Yul".to_owned(),
                    sources,
                    settings,
                }
            }
            other => unreachable!("Unexpected `SourceCodeData` variant: {other:?}"),
        };

        Ok(SolcInput {
            standard_json,
            contract_name,
            file_name,
        })
    }
}

#[async_trait]
impl Compiler<SolcInput> for Solc {
    async fn compile(
        self: Box<Self>,
        input: SolcInput,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let mut command = tokio::process::Command::new(&self.path);
        let mut child = command
            .arg("--standard-json")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed spawning solc")?;
        let stdin = child.stdin.as_mut().unwrap();
        let content = serde_json::to_vec(&input.standard_json)
            .context("cannot encode standard JSON input for solc")?;
        stdin
            .write_all(&content)
            .await
            .context("failed writing standard JSON to solc stdin")?;
        stdin
            .flush()
            .await
            .context("failed flushing standard JSON to solc")?;

        let output = child.wait_with_output().await.context("solc failed")?;
        if output.status.success() {
            let output = serde_json::from_slice(&output.stdout)
                .context("zksolc output is not valid JSON")?;
            parse_standard_json_output(&output, input.contract_name, input.file_name, true)
        } else {
            Err(ContractVerifierError::CompilerError(
                "solc",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }
}
