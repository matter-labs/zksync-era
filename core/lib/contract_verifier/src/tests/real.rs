//! Tests using real compiler toolchains. Should be prepared by calling `zkstack contract-verifier init`
//! with at least one `solc` and `zksolc` version. If there are no compilers, the tests will be ignored
//! unless the `RUN_CONTRACT_VERIFICATION_TEST` env var is set to `true`, in which case the tests will fail.
//!
//! You can install the compilers to run these tests with the following command:
//! ```
//! zkstack contract-verifier init --zksolc-version=v1.5.10 --zkvyper-version=v1.5.4 --solc-version=0.8.26 --vyper-version=v0.3.10 --era-vm-solc-version=0.8.26-1.0.1 --only
//! ```

use std::{env, sync::Arc, time::Duration};

use assert_matches::assert_matches;
use zksync_types::{
    bytecode::validate_bytecode, contract_verification::contract_identifier::DetectedMetadata,
};

use super::*;

#[derive(Debug, Clone, Copy)]
enum Toolchain {
    Solidity,
    Vyper,
}

impl Toolchain {
    const ALL: [Self; 2] = [Self::Solidity, Self::Vyper];
}

// The tests may expect specific compiler versions (e.g. contracts won't compile with Vyper 0.4.0),
// so we hardcode versions.
const ZKSOLC_VERSION: &str = "v1.5.10";
const ERA_VM_SOLC_VERSION: &str = "0.8.26-1.0.2";
const SOLC_VERSION: &str = "0.8.26";
const VYPER_VERSION: &str = "v0.3.10";
const ZKVYPER_VERSION: &str = "v1.5.4";

#[derive(Debug, Clone)]
struct TestCompilerVersions {
    solc: String,
    eravm_solc: String,
    zksolc: String,
    vyper: String,
    zkvyper: String,
}

impl TestCompilerVersions {
    fn new(versions: SupportedCompilerVersions) -> anyhow::Result<Self> {
        // Stored compilers for our fork are prefixed with `zkVM-`.
        let eravm_solc = format!("zkVM-{ERA_VM_SOLC_VERSION}");
        // Stored compilers for vyper do not have `v` prefix.
        let vyper = VYPER_VERSION.strip_prefix("v").unwrap().to_owned();
        anyhow::ensure!(
            versions.solc.contains(SOLC_VERSION),
            "Expected solc version {SOLC_VERSION} to be installed, but it is not"
        );
        anyhow::ensure!(
            versions.solc.contains(&eravm_solc),
            "Expected era-vm solc version {ERA_VM_SOLC_VERSION} to be installed, but it is not"
        );
        anyhow::ensure!(
            versions.zksolc.contains(ZKSOLC_VERSION),
            "Expected zksolc version {ZKSOLC_VERSION} to be installed, but it is not"
        );
        anyhow::ensure!(
            versions.vyper.contains(&vyper),
            "Expected vyper version {VYPER_VERSION} to be installed, but it is not"
        );
        anyhow::ensure!(
            versions.zkvyper.contains(ZKVYPER_VERSION),
            "Expected zkvyper version {ZKVYPER_VERSION} to be installed, but it is not"
        );

        Ok(Self {
            solc: SOLC_VERSION.to_owned(),
            eravm_solc,
            zksolc: ZKSOLC_VERSION.to_owned(),
            vyper,
            zkvyper: ZKVYPER_VERSION.to_owned(),
        })
    }

    fn zksolc(self) -> ZkCompilerVersions {
        ZkCompilerVersions {
            base: self.eravm_solc,
            zk: self.zksolc,
        }
    }

    fn solc_for_api(self, bytecode_kind: BytecodeMarker) -> CompilerVersions {
        CompilerVersions::Solc {
            compiler_solc_version: match bytecode_kind {
                BytecodeMarker::Evm => self.solc,
                BytecodeMarker::EraVm => self.eravm_solc,
            },
            compiler_zksolc_version: match bytecode_kind {
                BytecodeMarker::Evm => None,
                BytecodeMarker::EraVm => Some(self.zksolc),
            },
        }
    }

    fn zkvyper(self) -> ZkCompilerVersions {
        ZkCompilerVersions {
            base: self.vyper,
            zk: self.zkvyper,
        }
    }

    fn vyper_for_api(self, bytecode_kind: BytecodeMarker) -> CompilerVersions {
        CompilerVersions::Vyper {
            compiler_vyper_version: self.vyper,
            compiler_zkvyper_version: match bytecode_kind {
                BytecodeMarker::Evm => None,
                BytecodeMarker::EraVm => Some(self.zkvyper),
            },
        }
    }
}

async fn checked_env_resolver() -> anyhow::Result<(EnvCompilerResolver, TestCompilerVersions)> {
    let compiler_resolver = EnvCompilerResolver::default();
    let supported_compilers = compiler_resolver.supported_versions().await?;
    Ok((
        compiler_resolver,
        TestCompilerVersions::new(supported_compilers)?,
    ))
}

fn assert_no_compilers_expected(err: anyhow::Error) {
    let error_message = format!(
        "Expected pre-installed compilers since `RUN_CONTRACT_VERIFICATION_TEST=true`, but at least one compiler is not installed.\n \
        Detail: {}\n\n \
        Use the following command to install compilers:\n \
        zkstack contract-verifier init --zksolc-version={} --zkvyper-version={} --solc-version={} --vyper-version={} --era-vm-solc-version={} --only",
        err, ZKSOLC_VERSION, ZKVYPER_VERSION, SOLC_VERSION, VYPER_VERSION, ERA_VM_SOLC_VERSION
    );

    assert_ne!(
        env::var("RUN_CONTRACT_VERIFICATION_TEST").ok().as_deref(),
        Some("true"),
        "{error_message}"
    );
    println!("At least one compiler is not found, skipping the test");
}

/// Simplifies initializing real compiler resolver in tests.
macro_rules! real_resolver {
    () => {
        match checked_env_resolver().await {
            Ok(resolver_and_versions) => resolver_and_versions,
            Err(err) => {
                assert_no_compilers_expected(err);
                return;
            }
        }
    };
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn using_real_zksolc(specify_contract_file: bool) {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let compiler = compiler_resolver
        .resolve_zksolc(&supported_compilers.clone().zksolc())
        .await
        .unwrap();
    let mut req = VerificationIncomingRequest {
        compiler_versions: supported_compilers.solc_for_api(BytecodeMarker::EraVm),
        ..test_request(Address::repeat_byte(1), COUNTER_CONTRACT)
    };
    if specify_contract_file {
        set_multi_file_solc_input(&mut req);
    }

    let input = ZkSolc::build_input(req).unwrap();
    let output = compiler.compile(input).await.unwrap();

    validate_bytecode(&output.bytecode).unwrap();
    assert_eq!(output.abi, counter_contract_abi());
}

fn set_multi_file_solc_input(req: &mut VerificationIncomingRequest) {
    let input = serde_json::json!({
        "language": "Solidity",
        "sources": {
            "contracts/test.sol": {
                "content": COUNTER_CONTRACT,
            },
        },
        "settings": {
            "optimizer": { "enabled": true },
        },
    });
    let serde_json::Value::Object(input) = input else {
        unreachable!();
    };
    req.source_code_data = SourceCodeData::StandardJsonInput(input);
    req.contract_name = "contracts/test.sol:Counter".to_owned();
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn using_standalone_solc(specify_contract_file: bool) {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let version = &supported_compilers.solc;
    let compiler = compiler_resolver.resolve_solc(version).await.unwrap();
    let mut req = VerificationIncomingRequest {
        compiler_versions: CompilerVersions::Solc {
            compiler_solc_version: version.clone(),
            compiler_zksolc_version: None,
        },
        ..test_request(Address::repeat_byte(1), COUNTER_CONTRACT)
    };
    if specify_contract_file {
        set_multi_file_solc_input(&mut req);
    }

    let input = Solc::build_input(req).unwrap();
    let output = compiler.compile(input).await.unwrap();

    assert!(output.deployed_bytecode.is_some());
    assert_eq!(output.abi, counter_contract_abi());
}

#[test_casing(3, [(Some(100), None), (None, Some("shanghai")), (Some(200), Some("paris"))])]
#[tokio::test]
async fn using_standalone_solc_with_custom_settings(runs: Option<u16>, evm_version: Option<&str>) {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let version = &supported_compilers.solc;
    let compiler = compiler_resolver.resolve_solc(version).await.unwrap();
    let mut req = VerificationIncomingRequest {
        compiler_versions: CompilerVersions::Solc {
            compiler_solc_version: version.clone(),
            compiler_zksolc_version: None,
        },
        ..test_request(Address::repeat_byte(1), COUNTER_CONTRACT)
    };
    req.evm_specific.evm_version = evm_version.map(|s| s.to_owned());
    req.evm_specific.optimizer_runs = runs;

    let input = Solc::build_input(req).unwrap();
    let output = compiler.compile(input).await.unwrap();

    assert!(output.deployed_bytecode.is_some());
    assert_eq!(output.abi, counter_contract_abi());
}

#[tokio::test]
async fn using_standalone_solc_with_incorrect_evm_version_fails() {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let version = &supported_compilers.solc;
    let compiler = compiler_resolver.resolve_solc(version).await.unwrap();
    let mut req = VerificationIncomingRequest {
        compiler_versions: CompilerVersions::Solc {
            compiler_solc_version: version.clone(),
            compiler_zksolc_version: None,
        },
        ..test_request(Address::repeat_byte(1), COUNTER_CONTRACT)
    };
    req.evm_specific.evm_version = Some("not-a-real-version".to_owned());

    let input = Solc::build_input(req).unwrap();
    let output = compiler.compile(input).await;

    assert_matches!(output, Err(ContractVerifierError::CompilationError(_)));
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn using_zksolc_with_abstract_contract(specify_contract_file: bool) {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let compiler = compiler_resolver
        .resolve_zksolc(&supported_compilers.clone().zksolc())
        .await
        .unwrap();
    let (source_code_data, contract_name) = if specify_contract_file {
        let input = serde_json::json!({
            "language": "Solidity",
            "sources": {
                "contracts/test.sol": {
                    "content": COUNTER_CONTRACT_WITH_INTERFACE,
                },
            },
            "settings": {
                "optimizer": { "enabled": true },
            },
        });
        let serde_json::Value::Object(input) = input else {
            unreachable!();
        };
        (
            SourceCodeData::StandardJsonInput(input),
            "contracts/test.sol:ICounter",
        )
    } else {
        (
            SourceCodeData::SolSingleFile(COUNTER_CONTRACT_WITH_INTERFACE.to_owned()),
            "ICounter",
        )
    };

    let req = VerificationIncomingRequest {
        contract_address: Address::repeat_byte(1),
        compiler_versions: supported_compilers.solc_for_api(BytecodeMarker::EraVm),
        optimization_used: true,
        optimizer_mode: None,
        constructor_arguments: Default::default(),
        is_system: false,
        source_code_data,
        contract_name: contract_name.to_owned(),
        force_evmla: false,
        evm_specific: Default::default(),
    };

    let input = ZkSolc::build_input(req).unwrap();
    let err = compiler.compile(input).await.unwrap_err();
    assert_matches!(
        err,
        ContractVerifierError::AbstractContract(name) if name == "ICounter"
    );
}

fn test_yul_request(compiler_versions: CompilerVersions) -> VerificationIncomingRequest {
    VerificationIncomingRequest {
        contract_address: Default::default(),
        source_code_data: SourceCodeData::YulSingleFile(EMPTY_YUL_CONTRACT.to_owned()),
        contract_name: "Empty".to_owned(),
        compiler_versions,
        optimization_used: true,
        optimizer_mode: None,
        constructor_arguments: Default::default(),
        is_system: false,
        force_evmla: false,
        evm_specific: Default::default(),
    }
}

#[tokio::test]
async fn compiling_yul_with_zksolc() {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let version = supported_compilers.clone().zksolc();
    let compiler = compiler_resolver.resolve_zksolc(&version).await.unwrap();
    let req = test_yul_request(supported_compilers.solc_for_api(BytecodeMarker::EraVm));
    let input = ZkSolc::build_input(req).unwrap();
    let output = compiler.compile(input).await.unwrap();
    let identifier =
        ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, output.deployed_bytecode());

    assert!(!output.bytecode.is_empty());
    assert!(output.deployed_bytecode.is_none());
    assert_eq!(output.abi, serde_json::json!([]));
    assert_matches!(
        identifier.detected_metadata,
        Some(DetectedMetadata::Keccak256)
    );
}

#[tokio::test]
async fn compiling_standalone_yul() {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let version = &supported_compilers.solc;
    let compiler = compiler_resolver.resolve_solc(version).await.unwrap();
    let req = test_yul_request(CompilerVersions::Solc {
        compiler_solc_version: version.clone(),
        compiler_zksolc_version: None,
    });
    let input = Solc::build_input(req).unwrap();
    let output = compiler.compile(input).await.unwrap();
    let identifier =
        ContractIdentifier::from_bytecode(BytecodeMarker::Evm, output.deployed_bytecode());

    assert!(!output.bytecode.is_empty());
    assert_ne!(output.deployed_bytecode.unwrap(), output.bytecode);
    assert_eq!(output.abi, serde_json::json!([]));
    assert_matches!(
        identifier.detected_metadata,
        None,
        "No metadata for compiler yul for EVM"
    );
}

fn test_vyper_request(
    filename: &str,
    contract_name: &str,
    supported_compilers: TestCompilerVersions,
    bytecode_kind: BytecodeMarker,
) -> VerificationIncomingRequest {
    VerificationIncomingRequest {
        contract_address: Address::repeat_byte(1),
        source_code_data: SourceCodeData::VyperMultiFile(HashMap::from([(
            filename.to_owned(),
            COUNTER_VYPER_CONTRACT.to_owned(),
        )])),
        contract_name: contract_name.to_owned(),
        compiler_versions: supported_compilers.vyper_for_api(bytecode_kind),
        optimization_used: true,
        optimizer_mode: None,
        constructor_arguments: Default::default(),
        is_system: false,
        force_evmla: false,
        evm_specific: Default::default(),
    }
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn using_real_zkvyper(specify_contract_file: bool) {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let compiler = compiler_resolver
        .resolve_zkvyper(&supported_compilers.clone().zkvyper())
        .await
        .unwrap();
    let (filename, contract_name) = if specify_contract_file {
        ("contracts/Counter.vy", "contracts/Counter.vy:Counter")
    } else {
        ("Counter", "Counter")
    };
    let req = test_vyper_request(
        filename,
        contract_name,
        supported_compilers,
        BytecodeMarker::EraVm,
    );
    let input = VyperInput::new(req).unwrap();
    let output = compiler.compile(input).await.unwrap();
    let identifier =
        ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, output.deployed_bytecode());

    validate_bytecode(&output.bytecode).unwrap();
    assert_eq!(output.abi, without_internal_types(counter_contract_abi()));
    assert_matches!(
        identifier.detected_metadata,
        Some(DetectedMetadata::Keccak256)
    );
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn using_standalone_vyper(specify_contract_file: bool) {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let version = &supported_compilers.vyper;
    let compiler = compiler_resolver.resolve_vyper(version).await.unwrap();
    let (filename, contract_name) = if specify_contract_file {
        ("contracts/Counter.vy", "contracts/Counter.vy:Counter")
    } else {
        ("Counter.vy", "Counter")
    };
    let req = test_vyper_request(
        filename,
        contract_name,
        supported_compilers,
        BytecodeMarker::Evm,
    );
    let input = VyperInput::new(req).unwrap();
    let output = compiler.compile(input).await.unwrap();
    let identifier =
        ContractIdentifier::from_bytecode(BytecodeMarker::Evm, output.deployed_bytecode());

    assert!(output.deployed_bytecode.is_some());
    assert_eq!(output.abi, without_internal_types(counter_contract_abi()));
    // Vyper does not provide metadata for bytecode.
    assert_matches!(identifier.detected_metadata, None);
}

#[tokio::test]
async fn using_standalone_vyper_without_optimization() {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let version = &supported_compilers.vyper;
    let compiler = compiler_resolver.resolve_vyper(version).await.unwrap();
    let mut req = test_vyper_request(
        "counter.vy",
        "counter",
        supported_compilers,
        BytecodeMarker::Evm,
    );
    req.optimization_used = false;
    let input = VyperInput::new(req).unwrap();
    let output = compiler.compile(input).await.unwrap();
    let identifier =
        ContractIdentifier::from_bytecode(BytecodeMarker::Evm, output.deployed_bytecode());

    assert!(output.deployed_bytecode.is_some());
    assert_eq!(output.abi, without_internal_types(counter_contract_abi()));
    // Vyper does not provide metadata for bytecode.
    assert_matches!(identifier.detected_metadata, None);
}

#[tokio::test]
async fn using_standalone_vyper_with_code_size_optimization() {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let version = &supported_compilers.vyper;
    let compiler = compiler_resolver.resolve_vyper(version).await.unwrap();
    let mut req = test_vyper_request(
        "counter.vy",
        "counter",
        supported_compilers,
        BytecodeMarker::Evm,
    );
    req.optimization_used = true;
    req.optimizer_mode = Some("codesize".to_owned());
    let input = VyperInput::new(req).unwrap();
    let output = compiler.compile(input).await.unwrap();

    assert!(output.deployed_bytecode.is_some());
    assert_eq!(output.abi, without_internal_types(counter_contract_abi()));
}

#[tokio::test]
async fn using_standalone_vyper_with_bogus_optimization() {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let version = &supported_compilers.vyper;
    let compiler = compiler_resolver.resolve_vyper(version).await.unwrap();
    let mut req = test_vyper_request(
        "counter.vy",
        "counter",
        supported_compilers,
        BytecodeMarker::Evm,
    );
    req.optimization_used = true;
    req.optimizer_mode = Some("???".to_owned());
    let input = VyperInput::new(req).unwrap();
    let err = compiler.compile(input).await.unwrap_err();

    let ContractVerifierError::CompilationError(serde_json::Value::Array(errors)) = err else {
        panic!("unexpected error: {err:?}");
    };
    let has_opt_level_error = errors
        .iter()
        .any(|err| err.as_str().unwrap().contains("optimization level"));
    assert!(has_opt_level_error, "{errors:?}");
}

#[test_casing(4, Product((BYTECODE_KINDS, Toolchain::ALL)))]
#[tokio::test]
async fn using_real_compiler_in_verifier(bytecode_kind: BytecodeMarker, toolchain: Toolchain) {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let req = match toolchain {
        Toolchain::Solidity => VerificationIncomingRequest {
            compiler_versions: supported_compilers.clone().solc_for_api(bytecode_kind),
            ..test_request(Address::repeat_byte(1), COUNTER_CONTRACT)
        },
        Toolchain::Vyper => VerificationIncomingRequest {
            compiler_versions: supported_compilers.clone().vyper_for_api(bytecode_kind),
            source_code_data: SourceCodeData::VyperMultiFile(HashMap::from([(
                "Counter.vy".to_owned(),
                COUNTER_VYPER_CONTRACT.to_owned(),
            )])),
            ..test_request(Address::repeat_byte(1), COUNTER_CONTRACT)
        },
    };
    let address = Address::repeat_byte(1);
    let output = match (bytecode_kind, toolchain) {
        (BytecodeMarker::EraVm, Toolchain::Solidity) => {
            let compiler = compiler_resolver
                .resolve_zksolc(&supported_compilers.zksolc())
                .await
                .unwrap();
            let input = ZkSolc::build_input(req.clone()).unwrap();
            compiler.compile(input).await.unwrap()
        }
        (BytecodeMarker::Evm, Toolchain::Solidity) => {
            let solc_version = &supported_compilers.solc;
            let compiler = compiler_resolver.resolve_solc(solc_version).await.unwrap();
            let input = Solc::build_input(req.clone()).unwrap();
            compiler.compile(input).await.unwrap()
        }
        (_, Toolchain::Vyper) => {
            let compiler = match bytecode_kind {
                BytecodeMarker::EraVm => compiler_resolver
                    .resolve_zkvyper(&supported_compilers.zkvyper())
                    .await
                    .unwrap(),
                BytecodeMarker::Evm => compiler_resolver
                    .resolve_vyper(&supported_compilers.vyper)
                    .await
                    .unwrap(),
            };
            let input = VyperInput::new(req.clone()).unwrap();
            compiler.compile(input).await.unwrap()
        }
    };
    let identifier = ContractIdentifier::from_bytecode(bytecode_kind, output.deployed_bytecode());

    match (bytecode_kind, toolchain) {
        (BytecodeMarker::Evm, Toolchain::Vyper) => {
            assert!(
                identifier.detected_metadata.is_none(),
                "No metadata for EVM Vyper"
            );
        }
        (BytecodeMarker::Evm, Toolchain::Solidity) => {
            assert_matches!(
                identifier.detected_metadata,
                Some(DetectedMetadata::Cbor { .. }),
                "Cbor metadata for EVM Solidity by default"
            );
        }
        (BytecodeMarker::EraVm, _) => {
            assert_matches!(
                identifier.detected_metadata,
                Some(DetectedMetadata::Keccak256),
                "Keccak256 metadata for EraVM by default"
            );
        }
    }

    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;
    match bytecode_kind {
        BytecodeMarker::EraVm => {
            mock_deployment(&mut storage, address, output.bytecode.clone(), &[]).await;
        }
        BytecodeMarker::Evm => {
            mock_evm_deployment(
                &mut storage,
                address,
                output.bytecode.clone(),
                output.deployed_bytecode(),
                &[],
            )
            .await;
        }
    }
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(compiler_resolver),
        false,
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    assert_request_success(&mut storage, request_id, address, &output.bytecode, &[]).await;
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn using_zksolc_partial_match(use_cbor: bool) {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let mut req: VerificationIncomingRequest = VerificationIncomingRequest {
        compiler_versions: supported_compilers
            .clone()
            .solc_for_api(BytecodeMarker::EraVm),
        ..test_request(Address::repeat_byte(1), COUNTER_CONTRACT)
    };
    let hash_type = if use_cbor { "ipfs" } else { "keccak256" };
    // We need to manually construct the input, since `SolSingleFile` doesn't let us specify metadata hash type.
    // Note: prior to 1.5.7 field was named `bytecodeHash`.
    req.source_code_data = SourceCodeData::StandardJsonInput(
        serde_json::json!({
            "language": "Solidity",
            "sources": {
                "Counter.sol": {
                    "content": COUNTER_CONTRACT,
                },
            },
            "settings": {
                "outputSelection": {
                    "*": {
                        "": [ "abi" ],
                        "*": [ "abi" ]
                    }
                },
                "isSystem": false,
                "forceEvmla": false,
                "metadata": {
                    "hashType": hash_type
                },
                "optimizer": {
                    "enabled": true
                }
            }
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    let contract_name = req.contract_name.clone();
    let address = Address::repeat_byte(1);
    let compiler = compiler_resolver
        .resolve_zksolc(&supported_compilers.clone().zksolc())
        .await
        .unwrap();
    let input_for_request = ZkSolc::build_input(req.clone()).unwrap();

    let output_for_request = compiler.compile(input_for_request).await.unwrap();
    let identifier_for_request = ContractIdentifier::from_bytecode(
        BytecodeMarker::EraVm,
        output_for_request.deployed_bytecode(),
    );

    // Now prepare data for contract verification storage (with different metadata).
    let compiler = compiler_resolver
        .resolve_zksolc(&supported_compilers.zksolc())
        .await
        .unwrap();
    let mut input_for_storage = ZkSolc::build_input(req.clone()).unwrap();
    // Change the source file name.
    if let ZkSolcInput::StandardJson {
        input, file_name, ..
    } = &mut input_for_storage
    {
        let source = input
            .sources
            .remove(&format!("{contract_name}.sol"))
            .unwrap();
        let new_file_name = "random_name.sol".to_owned();
        input.sources.insert(new_file_name.clone(), source);
        *file_name = new_file_name;
        if use_cbor {
            input.settings.other.as_object_mut().unwrap().insert(
                "metadata".to_string(),
                serde_json::json!({ "hashType": "ipfs"}),
            );
        }
    } else {
        panic!("unexpected input: {input_for_storage:?}");
    }

    let output_for_storage = compiler.compile(input_for_storage).await.unwrap();
    let identifier_for_storage = ContractIdentifier::from_bytecode(
        BytecodeMarker::EraVm,
        output_for_storage.deployed_bytecode(),
    );

    assert_eq!(
        identifier_for_request.matches(&ContractIdentifier::from_bytecode(
            BytecodeMarker::EraVm,
            output_for_storage.deployed_bytecode()
        )),
        Match::Partial,
        "must be a partial match (1)"
    );
    assert_eq!(
        identifier_for_storage.matches(&ContractIdentifier::from_bytecode(
            BytecodeMarker::EraVm,
            output_for_request.deployed_bytecode()
        )),
        Match::Partial,
        "must be a partial match (2)"
    );
    if use_cbor {
        assert_matches!(
            identifier_for_request.detected_metadata,
            Some(DetectedMetadata::Cbor { .. })
        );
        assert_matches!(
            identifier_for_storage.detected_metadata,
            Some(DetectedMetadata::Cbor { .. })
        );
    } else {
        assert_matches!(
            identifier_for_request.detected_metadata,
            Some(DetectedMetadata::Keccak256)
        );
        assert_matches!(
            identifier_for_storage.detected_metadata,
            Some(DetectedMetadata::Keccak256)
        );
    }

    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;
    mock_deployment(
        &mut storage,
        address,
        output_for_storage.bytecode.clone(),
        &[],
    )
    .await;
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(compiler_resolver),
        false,
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    assert_request_success(
        &mut storage,
        request_id,
        address,
        &output_for_request.bytecode,
        &[VerificationProblem::IncorrectMetadata],
    )
    .await;
}

#[test_casing(2, BYTECODE_KINDS)]
#[tokio::test]
async fn compilation_errors(bytecode_kind: BytecodeMarker) {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let address = Address::repeat_byte(1);
    let req = VerificationIncomingRequest {
        compiler_versions: supported_compilers.solc_for_api(bytecode_kind),
        source_code_data: SourceCodeData::SolSingleFile("contract ???".to_owned()),
        ..test_request(Address::repeat_byte(1), COUNTER_CONTRACT)
    };

    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;
    match bytecode_kind {
        BytecodeMarker::EraVm => {
            mock_deployment(&mut storage, address, vec![0; 32], &[]).await;
        }
        BytecodeMarker::Evm => {
            mock_evm_deployment(&mut storage, address, vec![3; 20], &[5; 10], &[]).await;
        }
    }

    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(compiler_resolver),
        false,
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    let status = storage
        .contract_verification_dal()
        .get_verification_request_status(request_id)
        .await
        .unwrap()
        .expect("no status");
    assert_eq!(status.status, "failed");
    let compilation_errors = status.compilation_errors.unwrap();
    assert!(!compilation_errors.is_empty());
    let has_parser_error = compilation_errors
        .iter()
        .any(|err| err.contains("ParserError") && err.contains("Counter.sol"));
    assert!(has_parser_error, "{compilation_errors:?}");
}
