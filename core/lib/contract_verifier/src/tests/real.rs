//! Tests using real compiler toolchains. Should be prepared by calling `zkstack contract-verifier init`
//! with at least one `solc` and `zksolc` version. If there are no compilers, the tests will be ignored
//! unless the `RUN_CONTRACT_VERIFICATION_TEST` env var is set to `true`, in which case the tests will fail.

use std::{env, sync::Arc, time::Duration};

use zksync_utils::bytecode::validate_bytecode;

use super::*;

fn assert_no_compilers_expected() {
    assert_ne!(
        env::var("RUN_CONTRACT_VERIFICATION_TEST").ok().as_deref(),
        Some("true"),
        "Expected pre-installed compilers since `RUN_CONTRACT_VERIFICATION_TEST=true`, but they are not installed. \
         Use `zkstack contract-verifier init` to install compilers"
    );
    println!("No compilers found, skipping the test");
}

#[tokio::test]
async fn using_real_compiler() {
    let Some((compiler_resolver, supported_compilers)) = checked_env_resolver().await else {
        assert_no_compilers_expected();
        return;
    };

    let versions = CompilerVersions::Solc {
        compiler_zksolc_version: supported_compilers.zksolc[0].clone(),
        compiler_solc_version: supported_compilers.solc[0].clone(),
    };
    let compiler = compiler_resolver.resolve_solc(&versions).await.unwrap();
    let req = VerificationIncomingRequest {
        compiler_versions: versions,
        ..test_request(Address::repeat_byte(1))
    };
    let input = ContractVerifier::build_zksolc_input(req).unwrap();
    let output = compiler.compile(input).await.unwrap();

    validate_bytecode(&output.bytecode).unwrap();
    assert_eq!(output.abi, counter_contract_abi());
}

#[tokio::test]
async fn using_real_compiler_in_verifier() {
    let Some((compiler_resolver, supported_compilers)) = checked_env_resolver().await else {
        assert_no_compilers_expected();
        return;
    };

    let versions = CompilerVersions::Solc {
        compiler_zksolc_version: supported_compilers.zksolc[0].clone(),
        compiler_solc_version: supported_compilers.solc[0].clone(),
    };
    let address = Address::repeat_byte(1);
    let compiler = compiler_resolver.resolve_solc(&versions).await.unwrap();
    let req = VerificationIncomingRequest {
        compiler_versions: versions,
        ..test_request(Address::repeat_byte(1))
    };
    let input = ContractVerifier::build_zksolc_input(req.clone()).unwrap();
    let output = compiler.compile(input).await.unwrap();

    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;
    mock_deployment(&mut storage, address, output.bytecode.clone()).await;
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

#[tokio::test]
async fn compilation_errors() {
    let Some((compiler_resolver, supported_compilers)) = checked_env_resolver().await else {
        assert_no_compilers_expected();
        return;
    };

    let versions = CompilerVersions::Solc {
        compiler_zksolc_version: supported_compilers.zksolc[0].clone(),
        compiler_solc_version: supported_compilers.solc[0].clone(),
    };
    let address = Address::repeat_byte(1);
    let req = VerificationIncomingRequest {
        compiler_versions: versions,
        source_code_data: SourceCodeData::SolSingleFile("contract ???".to_owned()),
        ..test_request(Address::repeat_byte(1))
    };

    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;
    mock_deployment(&mut storage, address, vec![0; 32]).await;

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
