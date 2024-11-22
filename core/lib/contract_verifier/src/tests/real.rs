//! Tests using real compiler toolchains. Should be prepared by calling `zkstack contract-verifier init`
//! with at least one `solc` and `zksolc` version. If there are no compilers, the tests will be ignored
//! unless the `RUN_CONTRACT_VERIFICATION_TEST` env var is set to `true`, in which case the tests will fail.

use std::{env, sync::Arc, time::Duration};

use assert_matches::assert_matches;
use zksync_types::bytecode::validate_bytecode;

use super::*;

#[derive(Debug, Clone, Copy)]
enum Toolchain {
    Solidity,
    Vyper,
}

impl Toolchain {
    const ALL: [Self; 2] = [Self::Solidity, Self::Vyper];
}

#[derive(Debug, Clone)]
struct TestCompilerVersions {
    solc: String,
    zksolc: String,
    vyper: String,
    zkvyper: String,
}

impl TestCompilerVersions {
    fn new(versions: SupportedCompilerVersions) -> Option<Self> {
        let solc = versions
            .solc
            .into_iter()
            .find(|ver| !ver.starts_with("zkVM"))?;
        Some(Self {
            solc,
            zksolc: versions.zksolc.into_iter().next()?,
            vyper: versions.vyper.into_iter().next()?,
            zkvyper: versions.zkvyper.into_iter().next()?,
        })
    }

    fn zksolc(self) -> ZkCompilerVersions {
        ZkCompilerVersions {
            base: self.solc,
            zk: self.zksolc,
        }
    }

    fn solc_for_api(self, bytecode_kind: BytecodeMarker) -> CompilerVersions {
        CompilerVersions::Solc {
            compiler_solc_version: self.solc,
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

async fn checked_env_resolver() -> Option<(EnvCompilerResolver, TestCompilerVersions)> {
    let compiler_resolver = EnvCompilerResolver::default();
    let supported_compilers = compiler_resolver.supported_versions().await.ok()?;
    Some((
        compiler_resolver,
        TestCompilerVersions::new(supported_compilers)?,
    ))
}

fn assert_no_compilers_expected() {
    assert_ne!(
        env::var("RUN_CONTRACT_VERIFICATION_TEST").ok().as_deref(),
        Some("true"),
        "Expected pre-installed compilers since `RUN_CONTRACT_VERIFICATION_TEST=true`, but they are not installed. \
         Use `zkstack contract-verifier init` to install compilers"
    );
    println!("No compilers found, skipping the test");
}

/// Simplifies initializing real compiler resolver in tests.
macro_rules! real_resolver {
    () => {
        match checked_env_resolver().await {
            Some(resolver_and_versions) => resolver_and_versions,
            None => {
                assert_no_compilers_expected();
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

    assert!(!output.bytecode.is_empty());
    assert!(output.deployed_bytecode.is_none());
    assert_eq!(output.abi, serde_json::json!([]));
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

    assert!(!output.bytecode.is_empty());
    assert_ne!(output.deployed_bytecode.unwrap(), output.bytecode);
    assert_eq!(output.abi, serde_json::json!([]));
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

    validate_bytecode(&output.bytecode).unwrap();
    assert_eq!(output.abi, without_internal_types(counter_contract_abi()));
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

    assert!(output.deployed_bytecode.is_some());
    assert_eq!(output.abi, without_internal_types(counter_contract_abi()));
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

    assert!(output.deployed_bytecode.is_some());
    assert_eq!(output.abi, without_internal_types(counter_contract_abi()));
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
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    assert_request_success(&mut storage, request_id, address, &output.bytecode).await;
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
