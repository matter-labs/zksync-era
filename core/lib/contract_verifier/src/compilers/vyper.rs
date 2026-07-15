use std::{collections::HashMap, mem, path::PathBuf, process::Stdio};

use anyhow::Context;
use tokio::io::AsyncWriteExt;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification::api::{
    CompilationArtifacts, SourceCodeData, VerificationIncomingRequest,
};

use crate::{
    compilers::{
        parse_standard_json_output, process_contract_name, sanitize_compiler_stderr,
        validate_source_paths, Settings, Source, StandardJson,
    },
    error::ContractVerifierError,
    resolver::Compiler,
};

#[derive(Debug)]
pub(crate) struct VyperInput {
    pub contract_name: String,
    pub file_name: String,
    pub sources: HashMap<String, String>,
    pub optimizer_mode: Option<String>,
}

impl VyperInput {
    pub fn new(req: VerificationIncomingRequest) -> Result<Self, ContractVerifierError> {
        let (file_name, contract_name) = process_contract_name(&req.contract_name, "vy");

        let sources = match req.source_code_data {
            SourceCodeData::VyperMultiFile(s) => s,
            other => unreachable!("unexpected `SourceCodeData` variant: {other:?}"),
        };
        // Validate path keys before writing files to the temp directory.
        let sources_as_map: HashMap<String, Source> = sources
            .iter()
            .map(|(k, v)| (k.clone(), Source { content: v.clone() }))
            .collect();
        validate_source_paths(&sources_as_map)?;
        Ok(Self {
            contract_name,
            file_name,
            sources,
            optimizer_mode: if req.optimization_used {
                req.optimizer_mode
            } else {
                // `none` mode is not the default mode (which is `gas`), so we must specify it explicitly here
                Some("none".to_owned())
            },
        })
    }

    fn take_standard_json(&mut self) -> StandardJson {
        let sources = mem::take(&mut self.sources);
        let sources = sources
            .into_iter()
            .map(|(name, content)| (name, Source { content }));

        StandardJson {
            language: "Vyper".to_owned(),
            sources: sources.collect(),
            other: serde_json::json!({}),
            settings: Settings {
                output_selection: Some(serde_json::json!({
                    "*": [ "abi", "evm.bytecode", "evm.deployedBytecode" ],
                })),
                other: serde_json::json!({
                    "optimize": self.optimizer_mode.as_deref(),
                }),
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
        mut input: VyperInput,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        // Create an empty temp dir and pass it as the only import search path (-p).
        // All sources are provided inline via the standard JSON `content` field, so
        // the compiler never needs to read from the filesystem.  Limiting the search
        // path to this empty dir means any import not covered by the sources map
        // will fail with "file not found" rather than resolving against the host FS.
        let compile_dir = tempfile::tempdir().context("failed to create temp dir for vyper")?;

        let mut command = tokio::process::Command::new(&self.path);
        let mut child = command
            .arg("--standard-json")
            .arg("-p")
            .arg(compile_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("cannot spawn vyper")?;
        let mut stdin = child.stdin.take().unwrap();
        let standard_json = input.take_standard_json();
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
            let output =
                serde_json::from_slice(&output.stdout).context("vyper output is not valid JSON")?;
            parse_standard_json_output(&output, input.contract_name, input.file_name, true)
        } else {
            Err(ContractVerifierError::CompilerError(
                "vyper",
                sanitize_compiler_stderr(&String::from_utf8_lossy(&output.stderr)),
            ))
        }
    }
}
