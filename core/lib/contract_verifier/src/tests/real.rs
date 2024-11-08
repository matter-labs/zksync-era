//! Tests using real compiler toolchains. Should be prepared by calling `zkstack contract-verifier init`
//! with at least one `solc` and `zksolc` version. If there are no compilers, the tests will be ignored
//! unless the `RUN_CONTRACT_VERIFICATION_TEST` env var is set to `true`, in which case the tests will fail.

use std::{env, sync::Arc, time::Duration};

use zksync_utils::bytecode::validate_bytecode;

use super::*;

#[derive(Debug)]
struct TestCompilerVersions {
    solc: String,
    zksolc: String,
}

impl TestCompilerVersions {
    fn new(mut versions: SupportedCompilerVersions) -> Option<Self> {
        let solc = versions
            .solc
            .into_iter()
            .find(|ver| !ver.starts_with("zkVM"))?;
        Some(Self {
            solc,
            zksolc: versions.zksolc.pop()?,
        })
    }

    fn for_zksolc(self) -> CompilerVersions {
        CompilerVersions::Solc {
            compiler_solc_version: self.solc,
            compiler_zksolc_version: self.zksolc,
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

#[tokio::test]
async fn using_real_compiler() {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let versions = supported_compilers.for_zksolc();
    let compiler = compiler_resolver.resolve_zksolc(&versions).await.unwrap();
    let req = VerificationIncomingRequest {
        compiler_versions: versions,
        ..test_request(Address::repeat_byte(1), COUNTER_CONTRACT)
    };
    let input = ZkSolc::build_input(req).unwrap();
    let output = compiler.compile(input).await.unwrap();

    validate_bytecode(&output.bytecode).unwrap();
    assert_eq!(output.abi, counter_contract_abi());
}

#[tokio::test]
async fn using_standalone_solc() {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let version = &supported_compilers.solc;
    let compiler = compiler_resolver.resolve_solc(version).await.unwrap();
    let req = VerificationIncomingRequest {
        compiler_versions: CompilerVersions::Solc {
            compiler_solc_version: version.clone(),
            compiler_zksolc_version: "1000.0.0".to_owned(), // not used
        },
        ..test_request(Address::repeat_byte(1), COUNTER_CONTRACT)
    };
    let input = Solc::build_input(req).unwrap();
    let output = compiler.compile(input).await.unwrap();

    assert!(output.deployed_bytecode.is_some());
    assert_eq!(output.abi, counter_contract_abi());
}

#[test_casing(2, BYTECODE_KINDS)]
#[tokio::test]
async fn using_real_compiler_in_verifier(bytecode_kind: BytecodeMarker) {
    let (compiler_resolver, supported_compilers) = real_resolver!();

    let versions = supported_compilers.for_zksolc();
    let req = VerificationIncomingRequest {
        compiler_versions: versions,
        ..test_request(Address::repeat_byte(1), COUNTER_CONTRACT)
    };
    let address = Address::repeat_byte(1);
    let output = match bytecode_kind {
        BytecodeMarker::EraVm => {
            let compiler = compiler_resolver
                .resolve_zksolc(&req.compiler_versions)
                .await
                .unwrap();
            let input = ZkSolc::build_input(req.clone()).unwrap();
            compiler.compile(input).await.unwrap()
        }
        BytecodeMarker::Evm => {
            let solc_version = req.compiler_versions.compiler_version();
            let compiler = compiler_resolver.resolve_solc(solc_version).await.unwrap();
            let input = Solc::build_input(req.clone()).unwrap();
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
        .add_contract_verification_request(req)
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

    let versions = supported_compilers.for_zksolc();
    let address = Address::repeat_byte(1);
    let req = VerificationIncomingRequest {
        compiler_versions: versions,
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
        .add_contract_verification_request(req)
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
