use std::{collections::HashMap, path::PathBuf};

use anyhow::Context;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification::api::{
    CompilationArtifacts, SourceCodeData, VerificationIncomingRequest,
};

use crate::{
    compilers::{
        parse_standard_json_input, parse_standard_json_output, process_contract_name,
        sanitize_compiler_stderr, validate_contract_target, CompilerFlavor, Optimizer, Settings,
        Source, StandardJson,
    },
    error::ContractVerifierError,
    process::run_compiler,
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
        validate_contract_target(&file_name, &contract_name)?;
        if req.is_system || req.force_evmla {
            return Err(ContractVerifierError::UnsupportedVerificationInput(
                "system mode and force-EVMLA are not accepted".to_owned(),
            ));
        }
        if req.optimizer_mode.is_some() {
            return Err(ContractVerifierError::UnsupportedVerificationInput(
                "optimizer mode is not supported by solc verification".to_owned(),
            ));
        }
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
                let optimizer_runs = req
                    .evm_specific
                    .optimizer_runs
                    .map(u32::try_from)
                    .transpose()
                    .map_err(|_| {
                        ContractVerifierError::UnsupportedVerificationInput(
                            "optimizer runs exceeds the allowed limit".to_owned(),
                        )
                    })?;
                let settings = Settings {
                    output_selection: Some(default_output_selection),
                    optimizer: Some(Optimizer {
                        enabled: Some(req.optimization_used),
                        runs: optimizer_runs,
                        mode: None,
                        ..Optimizer::default()
                    }),
                    evm_version: req.evm_specific.evm_version,
                    ..Settings::default()
                };

                StandardJson {
                    language: "Solidity".to_owned(),
                    sources,
                    settings,
                }
            }
            SourceCodeData::StandardJsonInput(map) => {
                let mut compiler_input = parse_standard_json_input(map, CompilerFlavor::Solc)?;
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
                    optimizer: Some(Optimizer {
                        enabled: Some(req.optimization_used),
                        ..Optimizer::default()
                    }),
                    ..Settings::default()
                };
                StandardJson {
                    language: "Yul".to_owned(),
                    sources,
                    settings,
                }
            }
            SourceCodeData::VyperMultiFile(_) => {
                return Err(ContractVerifierError::UnsupportedVerificationInput(
                    "Vyper verification is disabled".to_owned(),
                ));
            }
        };
        if standard_json.language == "Yul" {
            standard_json.validate_yul(CompilerFlavor::Solc)?;
        } else {
            standard_json.validate(CompilerFlavor::Solc)?;
        }

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

    fn standard_json_req(input: serde_json::Value, name: &str) -> VerificationIncomingRequest {
        VerificationIncomingRequest {
            contract_address: Default::default(),
            source_code_data: SourceCodeData::StandardJsonInput(input.as_object().unwrap().clone()),
            contract_name: name.to_owned(),
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
        }
    }

    #[test]
    fn build_input_rejects_source_url_references() {
        // A source may only be provided as inline `content`; any `urls` field (which solc would
        // resolve against the filesystem) must never reach the compiler.
        let input = serde_json::json!({
            "language": "Solidity",
            "sources": {
                "src/Test.sol": {
                    "content": "contract Test {}",
                    "urls": ["/some/host/path/Evil.sol"],
                },
            },
            "settings": {},
        });

        let err = Solc::build_input(standard_json_req(input, "src/Test.sol:Test")).unwrap_err();
        assert!(
            matches!(err, ContractVerifierError::FailedToDeserializeInput),
            "url references must be rejected, got: {err:?}"
        );
    }

    #[test]
    fn build_input_rejects_source_without_content() {
        // A source with only `urls` and no inline `content` has no compilable body and is rejected
        // rather than being handed to the compiler for filesystem resolution.
        let input = serde_json::json!({
            "language": "Solidity",
            "sources": {
                "src/Test.sol": {
                    "urls": ["/some/host/path/Evil.sol"],
                },
            },
            "settings": {},
        });

        let err = Solc::build_input(standard_json_req(input, "src/Test.sol:Test")).unwrap_err();
        assert!(
            matches!(err, ContractVerifierError::FailedToDeserializeInput),
            "source without inline content must be rejected, got: {err:?}"
        );
    }

    #[test]
    fn build_input_erases_remappings() {
        let input = serde_json::json!({
            "language": "Solidity",
            "sources": {
                "src/Test.sol": {
                    "content": r#"import "@ext/Lib.sol"; contract Test {}"#,
                },
            },
            "settings": {
                "remappings": ["@ext/=../../../../outside/tree/"],
            },
        });
        let req = VerificationIncomingRequest {
            contract_address: Default::default(),
            source_code_data: SourceCodeData::StandardJsonInput(input.as_object().unwrap().clone()),
            contract_name: "src/Test.sol:Test".to_owned(),
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

        let input = Solc::build_input(req).unwrap();
        let serialized = serde_json::to_value(input.standard_json).unwrap();
        assert!(
            serialized.pointer("/settings/remappings").is_none(),
            "remappings must never reach the compiler: {serialized}"
        );
    }

    #[test]
    fn build_input_uses_evm_bytecode_outputs_for_evm_contracts() {
        let input = serde_json::json!({
            "language": "Solidity",
            "sources": {
                "src/Counter.sol": {
                    "content": "contract Counter {}",
                },
            },
            "settings": {
                "outputSelection": {
                    "*": {
                        "*": ["metadata"]
                    }
                },
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

        let built = Solc::build_input(req).expect("standard JSON input should build");
        let output_selection = built
            .standard_json
            .settings
            .output_selection
            .as_ref()
            .unwrap();
        let selected_outputs = output_selection["*"]["*"].as_array().unwrap();

        for expected_output in ["abi", "evm.bytecode", "evm.deployedBytecode"] {
            assert!(
                selected_outputs
                    .iter()
                    .any(|output| output.as_str() == Some(expected_output)),
                "missing {expected_output:?}: {selected_outputs:?}"
            );
        }
        assert!(
            !selected_outputs
                .iter()
                .any(|output| output.as_str() == Some("evm")),
            "standalone EVM solc should use explicit bytecode selectors: {selected_outputs:?}"
        );
    }

    #[test]
    fn build_input_retains_private_yul_capability() {
        let mut req = standard_json_req(serde_json::json!({}), "Empty");
        req.source_code_data = SourceCodeData::YulSingleFile("object \"Empty\" {}".to_owned());

        let input = Solc::build_input(req).unwrap();

        assert_eq!(input.standard_json.language, "Yul");
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
        // Resolve the binary to an absolute path so it stays locatable after `current_dir` is
        // switched to the empty working directory below.
        let solc_path = tokio::fs::canonicalize(&self.path)
            .await
            .context("failed to canonicalize solc path")?;

        let content = serde_json::to_vec(&input.standard_json)
            .context("cannot encode standard JSON input for solc")?;
        let mut command = tokio::process::Command::new(&solc_path);
        command
            .current_dir(compile_dir.path())
            .arg("--standard-json")
            .arg("--allow-paths")
            .arg(compile_dir.path());

        let output = run_compiler(&mut command, Some(&content)).await?;
        if output.status.success() {
            let output =
                serde_json::from_slice(&output.stdout).context("solc output is not valid JSON")?;
            parse_standard_json_output(&output, input.contract_name, input.file_name, true)
        } else {
            Err(ContractVerifierError::CompilerError(
                "solc",
                sanitize_compiler_stderr(&String::from_utf8_lossy(&output.stderr)),
            ))
        }
    }
}
