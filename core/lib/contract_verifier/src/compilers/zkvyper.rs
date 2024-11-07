use std::{collections::HashMap, fs::File, io::Write, path::Path, process::Stdio};

use anyhow::Context as _;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification_api::{
    CompilationArtifacts, SourceCodeData, VerificationIncomingRequest,
};

use crate::{
    error::ContractVerifierError,
    resolver::{Compiler, CompilerPaths},
};

#[derive(Debug)]
pub(crate) struct ZkVyperInput {
    pub contract_name: String,
    pub sources: HashMap<String, String>,
    pub optimizer_mode: Option<String>,
}

#[derive(Debug)]
pub(crate) struct ZkVyper {
    paths: CompilerPaths,
}

impl ZkVyper {
    pub fn new(paths: CompilerPaths) -> Self {
        Self { paths }
    }

    pub fn build_input(
        req: VerificationIncomingRequest,
    ) -> Result<ZkVyperInput, ContractVerifierError> {
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
        Ok(ZkVyperInput {
            contract_name,
            sources,
            optimizer_mode: req.optimizer_mode,
        })
    }

    fn parse_output(
        output: &serde_json::Value,
        contract_name: String,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let file_name = format!("{contract_name}.vy");
        let object = output
            .as_object()
            .context("Vyper output is not an object")?;
        for (path, artifact) in object {
            let path = Path::new(&path);
            if path.file_name().unwrap().to_str().unwrap() == file_name {
                let bytecode_str = artifact["bytecode"]
                    .as_str()
                    .context("bytecode is not a string")?;
                let bytecode_without_prefix =
                    bytecode_str.strip_prefix("0x").unwrap_or(bytecode_str);
                let bytecode =
                    hex::decode(bytecode_without_prefix).context("failed decoding bytecode")?;
                return Ok(CompilationArtifacts {
                    abi: artifact["abi"].clone(),
                    bytecode,
                    deployed_bytecode: None,
                });
            }
        }
        Err(ContractVerifierError::MissingContract(contract_name))
    }
}

#[async_trait]
impl Compiler<ZkVyperInput> for ZkVyper {
    async fn compile(
        self: Box<Self>,
        input: ZkVyperInput,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let mut command = tokio::process::Command::new(&self.paths.zk);
        if let Some(o) = input.optimizer_mode.as_ref() {
            command.arg("-O").arg(o);
        }
        command
            .arg("--vyper")
            .arg(&self.paths.base)
            .arg("-f")
            .arg("combined_json")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let temp_dir = tempfile::tempdir().context("failed creating temporary dir")?;
        for (mut name, content) in input.sources {
            if !name.ends_with(".vy") {
                name += ".vy";
            }
            let path = temp_dir.path().join(&name);
            if let Some(prefix) = path.parent() {
                std::fs::create_dir_all(prefix)
                    .with_context(|| format!("failed creating parent dir for `{name}`"))?;
            }
            let mut file = File::create(&path)
                .with_context(|| format!("failed creating file for `{name}`"))?;
            file.write_all(content.as_bytes())
                .with_context(|| format!("failed writing to `{name}`"))?;
            command.arg(path.into_os_string());
        }

        let child = command.spawn().context("cannot spawn zkvyper")?;
        let output = child.wait_with_output().await.context("zkvyper failed")?;
        if output.status.success() {
            let output = serde_json::from_slice(&output.stdout)
                .context("zkvyper output is not valid JSON")?;
            Self::parse_output(&output, input.contract_name)
        } else {
            Err(ContractVerifierError::CompilerError(
                "zkvyper",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }
}
