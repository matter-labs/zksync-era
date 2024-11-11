use std::{collections::HashMap, path::PathBuf, process::Stdio};

use anyhow::Context;
use tokio::io::AsyncWriteExt;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification_api::{
    CompilationArtifacts, SourceCodeData, VerificationIncomingRequest,
};

use super::{Settings, Source, StandardJson};
use crate::{error::ContractVerifierError, resolver::Compiler};

#[derive(Debug)]
pub(crate) struct VyperInput {
    pub contract_name: String,
    pub sources: HashMap<String, String>,
    pub optimizer_mode: Option<String>,
}

impl VyperInput {
    pub fn new(req: VerificationIncomingRequest) -> Result<Self, ContractVerifierError> {
        // Users may provide either just contract name or
        // source file name and contract name joined with ":".
        let contract_name = if let Some((_, contract_name)) = req.contract_name.rsplit_once(':') {
            contract_name.to_owned()
        } else {
            req.contract_name.clone()
        };

        let sources = match req.source_code_data {
            SourceCodeData::VyperMultiFile(s) => s,
            other => unreachable!("unexpected `SourceCodeData` variant: {other:?}"),
        };
        Ok(Self {
            contract_name,
            sources,
            optimizer_mode: req.optimizer_mode,
        })
    }

    fn standard_json(self) -> StandardJson {
        let sources = self
            .sources
            .into_iter()
            .map(|(name, content)| (name, Source { content }));

        StandardJson {
            language: "Vyper".to_owned(),
            sources: sources.collect(),
            settings: Settings {
                output_selection: Some(serde_json::json!({
                    "*": [ "abi", "evm.bytecode", "evm.deployedBytecode" ],
                     "": [ "abi", "evm.bytecode", "evm.deployedBytecode" ],
                })),
                other: serde_json::json!({}),
            },
        }
    }
}

#[derive(Debug)]
pub(crate) struct Vyper {
    path: PathBuf,
}

impl Vyper {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl Compiler<VyperInput> for Vyper {
    async fn compile(
        self: Box<Self>,
        input: VyperInput,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let mut command = tokio::process::Command::new(&self.path);
        let mut child = command
            .arg("--standard-json")
            .arg("-f")
            .arg("combined_json")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("cannot spawn vyper")?;
        let mut stdin = child.stdin.take().unwrap();
        let standard_json = input.standard_json();
        let content = serde_json::to_vec(&standard_json)
            .context("cannot encode standard JSON input for vyper")?;
        stdin
            .write_all(&content)
            .await
            .context("failed writing standard JSON to vyper stdin")?;
        stdin
            .flush()
            .await
            .context("failed flushing standard JSON to vyper")?;
        drop(stdin);

        let output = child.wait_with_output().await.context("vyper failed")?;
        if output.status.success() {
            let output: serde_json::Value =
                serde_json::from_slice(&output.stdout).context("vyper output is not valid JSON")?;
            panic!("{output:?}");
        } else {
            Err(ContractVerifierError::CompilerError(
                "vyper",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }
}
