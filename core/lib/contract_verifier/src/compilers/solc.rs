use std::{collections::HashMap, path::PathBuf, process::Stdio};

use anyhow::Context;
use tokio::io::AsyncWriteExt;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification::api::{
    CompilationArtifacts, SourceCodeData, VerificationIncomingRequest,
};

use crate::{
    compilers::{
        has_dangerous_imports, parse_standard_json_output, process_contract_name,
        sanitize_compiler_stderr, validate_source_paths, Settings, Source, StandardJson,
    },
    error::ContractVerifierError,
    resolver::Compiler,
};

// Here and below, fields are public for testing purposes.
#[derive(Debug)]
pub(crate) struct SolcInput {
    pub standard_json: StandardJson,
    pub contract_name: String,
    pub file_name: String,
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
        let (file_name, contract_name) = process_contract_name(&req.contract_name, "sol");
        let default_output_selection = serde_json::json!({
            "*": {
                "*": [ "abi", "evm.bytecode", "evm.deployedBytecode" ],
                 "": [ "abi", "evm.bytecode", "evm.deployedBytecode" ],
            }
        });

        let standard_json = match req.source_code_data {
            SourceCodeData::SolSingleFile(source_code) => {
                if has_dangerous_imports(&source_code) {
                    return Err(ContractVerifierError::InvalidSourcePath(
                        "import with absolute path".to_owned(),
                    ));
                }
                let source = Source {
                    content: source_code,
                };
                let sources = HashMap::from([(file_name.clone(), source)]);
                let mut settings = Settings {
                    output_selection: Some(default_output_selection),
                    other: serde_json::json!({
                        "optimizer": {
                            "enabled": req.optimization_used,
                        },
                    }),
                };
                if let Some(runs) = req.evm_specific.optimizer_runs {
                    settings.other["optimizer"]["runs"] = serde_json::json!(runs);
                }
                if let Some(evm_version) = req.evm_specific.evm_version {
                    settings.other["evmVersion"] = serde_json::json!(evm_version);
                }

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
                validate_source_paths(&compiler_input.sources)?;
                for source in compiler_input.sources.values() {
                    if has_dangerous_imports(&source.content) {
                        return Err(ContractVerifierError::InvalidSourcePath(
                            "import with absolute path".to_owned(),
                        ));
                    }
                }
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

#[cfg(test)]
mod tests {
    use zksync_types::contract_verification::api::CompilerVersions;

    use super::*;

    #[test]
    fn build_input_allows_relative_parent_imports_in_standard_json() {
        let input = serde_json::json!({
            "language": "Solidity",
            "sources": {
                "src/Counter.sol": {
                    "content": r#"
                        pragma solidity ^0.8.20;
                        import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";

                        contract Counter is OwnableUpgradeable {
                            function initialize(address owner) external initializer {
                                __Ownable_init(owner);
                            }
                        }
                    "#,
                },
                "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol": {
                    "content": r#"
                        pragma solidity ^0.8.20;
                        import "../utils/ContextUpgradeable.sol";
                        import "@openzeppelin/contracts/proxy/utils/Initializable.sol";

                        abstract contract OwnableUpgradeable is Initializable, ContextUpgradeable {
                            address private _owner;

                            function __Ownable_init(address initialOwner) internal onlyInitializing {
                                _owner = initialOwner;
                            }
                        }
                    "#,
                },
                "@openzeppelin/contracts-upgradeable/utils/ContextUpgradeable.sol": {
                    "content": r#"
                        pragma solidity ^0.8.20;
                        import "@openzeppelin/contracts/proxy/utils/Initializable.sol";

                        abstract contract ContextUpgradeable is Initializable {
                            function _msgSender() internal view virtual returns (address) {
                                return msg.sender;
                            }
                        }
                    "#,
                },
                "@openzeppelin/contracts/proxy/utils/Initializable.sol": {
                    "content": r#"
                        pragma solidity ^0.8.20;

                        abstract contract Initializable {
                            modifier initializer() {
                                _;
                            }

                            modifier onlyInitializing() {
                                _;
                            }
                        }
                    "#,
                },
            },
            "settings": {
                "optimizer": {
                    "enabled": true,
                },
            },
        });
        let req = VerificationIncomingRequest {
            contract_address: Default::default(),
            source_code_data: SourceCodeData::StandardJsonInput(input.as_object().unwrap().clone()),
            contract_name: "src/Counter.sol:Counter".to_owned(),
            compiler_versions: CompilerVersions::Solc {
                compiler_solc_version: "0.8.26".to_owned(),
                compiler_zksolc_version: None,
            },
            optimization_used: true,
            optimizer_mode: None,
            constructor_arguments: Default::default(),
            is_system: false,
            force_evmla: false,
            evm_specific: Default::default(),
        };

        let built = Solc::build_input(req).expect("relative parent imports should be allowed");

        assert_eq!(built.file_name, "src/Counter.sol");
        assert_eq!(built.contract_name, "Counter");
        assert!(built
            .standard_json
            .sources
            .contains_key("@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol"));
    }
}

#[async_trait]
impl Compiler<SolcInput> for Solc {
    async fn compile(
        self: Box<Self>,
        input: SolcInput,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        // Create an empty temp dir and restrict the compiler to it.
        // All sources are passed inline via the standard JSON `content` field, so
        // the compiler never needs to read from the filesystem.  Any import that is
        // not covered by the sources map will therefore fail with "File not found"
        // rather than silently reading an arbitrary host path.
        let compile_dir = tempfile::tempdir().context("failed to create temp dir for solc")?;

        let mut command = tokio::process::Command::new(&self.path);
        let mut child = command
            .arg("--standard-json")
            .arg("--allow-paths")
            .current_dir(compile_dir.path())
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
                sanitize_compiler_stderr(&String::from_utf8_lossy(&output.stderr)),
            ))
        }
    }
}
