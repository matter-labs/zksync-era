//! Tests for contract verification API server.

use std::{str, vec};

use test_casing::test_casing;
use utils::{mock_verification_info, MockApiClient, MockContractVerifier};
use zksync_types::{
    bytecode::BytecodeMarker,
    contract_verification::{
        api::VerificationProblem,
        etherscan::{
            EtherscanBoolean, EtherscanCodeFormat, EtherscanRequest, EtherscanRequestPayload,
            EtherscanVerificationRequest,
        },
    },
    Address,
};

use super::*;
use crate::{
    api_impl::ApiError,
    tests::utils::{mock_deploy_contract, prepare_storage, SOLC_VERSION, ZKSOLC_VERSION},
};

mod utils;

#[tokio::test]
async fn getting_compiler_versions() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let client = MockApiClient::new(pool.clone());
    prepare_storage(&mut storage).await;

    assert_eq!(client.zksolc_versions().await, &[ZKSOLC_VERSION]);
    assert_eq!(client.solc_versions().await, &[SOLC_VERSION]);
}

#[test_casing(2, [BytecodeMarker::EraVm, BytecodeMarker::Evm])]
#[tokio::test]
async fn submitting_request(bytecode_kind: BytecodeMarker) {
    let pool = ConnectionPool::test_pool().await;
    let contract_verifier = MockContractVerifier::new(pool.clone());
    let client = MockApiClient::new(pool.clone());
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let address = Address::repeat_byte(0x23);
    let verification_request = serde_json::json!({
        "contractAddress": address,
        "sourceCode": "contract Test {}",
        "contractName": "Test",
        "compilerZksolcVersion": match bytecode_kind {
            BytecodeMarker::EraVm => Some(ZKSOLC_VERSION),
            BytecodeMarker::Evm => None,
        },
        "compilerSolcVersion": SOLC_VERSION,
        "optimizationUsed": true,
    });

    // Contract is not deployed yet.
    client
        .assert_verification_request_error(&verification_request, ApiError::NoDeployedContract)
        .await;

    mock_deploy_contract(&mut storage, address, bytecode_kind).await;

    let id = client
        .send_verification_request(&verification_request)
        .await;
    assert_eq!(id, 1);

    // Duplicate request should not be created.
    client
        .assert_verification_request_error(&verification_request, ApiError::ActiveRequestExists(id))
        .await;

    // Pick up the request.
    contract_verifier
        .pick_up_next_request(id, &verification_request, bytecode_kind)
        .await;

    // Should be in progress now.
    let status = client.verification_status(id).await;
    assert_eq!(status.status, "in_progress");

    // Verify contract
    let verification_info = mock_verification_info(id, &verification_request);
    contract_verifier.verify_contract(verification_info).await;

    let status = client.verification_status(id).await;
    assert_eq!(status.status, "successful");

    // We should be able to fetch verification info
    let info = client.verification_info(address).await;
    assert_eq!(info.request.id, id);
    assert_eq!(info.artifacts.bytecode, vec![0xff, 32]);

    // No requests should be accepted after verification
    client
        .assert_verification_request_error(&verification_request, ApiError::AlreadyVerified)
        .await;
}

#[test_casing(2, [BytecodeMarker::EraVm, BytecodeMarker::Evm])]
#[tokio::test]
async fn submitting_etherscan_request(bytecode_kind: BytecodeMarker) {
    let pool = ConnectionPool::test_pool().await;
    let contract_verifier = MockContractVerifier::new(pool.clone());
    let client = MockApiClient::new(pool.clone());
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let address = Address::repeat_byte(0x23);
    let etherscan_verification_req = EtherscanVerificationRequest {
        code_format: EtherscanCodeFormat::SingleFile,
        source_code: "contract Test {}".to_string(),
        contract_address: address,
        contract_name: "Test".to_string(),
        zksolc_version: match bytecode_kind {
            BytecodeMarker::EraVm => Some(ZKSOLC_VERSION.to_string()),
            BytecodeMarker::Evm => None,
        },
        compiler_version: SOLC_VERSION.to_string(),
        optimization_used: Some(EtherscanBoolean::True),
        optimizer_mode: None,
        runs: None,
        evm_version: None,
        compiler_mode: None,
        is_system: Some(EtherscanBoolean::False),
        force_evmla: Some(EtherscanBoolean::False),
        constructor_arguments: String::default(),
    };
    let verification_json = serde_json::to_value(
        etherscan_verification_req
            .clone()
            .to_verification_request()
            .unwrap(),
    )
    .unwrap();

    let verification_request = EtherscanRequest {
        module: "contract".to_string(),
        payload: EtherscanRequestPayload::VerifySourceCode(etherscan_verification_req),
    };

    mock_deploy_contract(&mut storage, address, bytecode_kind).await;

    let etherscan_response = client.send_etherscan_request(&verification_request).await;

    assert_eq!(etherscan_response.status, "1");
    assert_eq!(etherscan_response.message, "OK");
    assert_eq!(etherscan_response.result, "1");

    let id = etherscan_response.result.parse::<usize>().unwrap();

    // Duplicate request should not be created.
    let etherscan_response = client.send_etherscan_request(&verification_request).await;

    assert_eq!(etherscan_response.status, "0");
    assert_eq!(etherscan_response.message, "NOTOK");
    assert_eq!(
        etherscan_response.result,
        "active request for this contract already exists, ID: 1"
    );

    // Pick up the request.
    contract_verifier
        .pick_up_next_request(id, &verification_json, bytecode_kind)
        .await;

    // Should be in progress now.
    let check_status_request = EtherscanRequest {
        module: "contract".to_string(),
        payload: EtherscanRequestPayload::CheckVerifyStatus {
            guid: id.to_string(),
        },
    };

    let etherscan_response = client.send_etherscan_request(&check_status_request).await;
    assert_eq!(etherscan_response.status, "0");
    assert_eq!(etherscan_response.message, "NOTOK");
    assert_eq!(etherscan_response.result, "Pending in queue");

    // Verify contract
    let verification_info = mock_verification_info(id, &verification_json);
    contract_verifier.verify_contract(verification_info).await;

    let etherscan_response = client.send_etherscan_request(&check_status_request).await;
    assert_eq!(etherscan_response.status, "1");
    assert_eq!(etherscan_response.message, "OK");
    assert_eq!(etherscan_response.result, "Pass - Verified");

    // We should be able to fetch verification info
    let info = client.verification_info(address).await;
    assert_eq!(info.request.id, id);
    assert_eq!(info.artifacts.bytecode, vec![0xff, 32]);

    // No requests should be accepted after verification
    let etherscan_response = client.send_etherscan_request(&verification_request).await;
    assert_eq!(etherscan_response.status, "0");
    assert_eq!(etherscan_response.message, "NOTOK");
    assert_eq!(
        etherscan_response.result,
        "Contract source code already verified"
    );
}

#[test_casing(2, [BytecodeMarker::EraVm, BytecodeMarker::Evm])]
#[tokio::test]
async fn partial_verification(bytecode_kind: BytecodeMarker) {
    let pool = ConnectionPool::test_pool().await;
    let contract_verifier = MockContractVerifier::new(pool.clone());
    let client = MockApiClient::new(pool.clone());
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let address = Address::repeat_byte(0x23);
    let verification_request = serde_json::json!({
        "contractAddress": address,
        "sourceCode": "contract Test {}",
        "contractName": "Test",
        "compilerZksolcVersion": match bytecode_kind {
            BytecodeMarker::EraVm => Some(ZKSOLC_VERSION),
            BytecodeMarker::Evm => None,
        },
        "compilerSolcVersion": SOLC_VERSION,
        "optimizationUsed": true,
    });

    mock_deploy_contract(&mut storage, address, bytecode_kind).await;

    let id = client
        .send_verification_request(&verification_request)
        .await;
    assert_eq!(id, 1);
    contract_verifier
        .pick_up_next_request(id, &verification_request, bytecode_kind)
        .await;

    // Verify contract (with a verification problem)
    let mut verification_info = mock_verification_info(id, &verification_request);
    verification_info.verification_problems = vec![VerificationProblem::IncorrectMetadata];
    contract_verifier
        .verify_contract(verification_info.clone())
        .await;
    let status = client.verification_status(id).await;
    assert_eq!(status.status, "successful");

    // We should be able to fetch verification info
    let info = client.verification_info(address).await;
    assert_eq!(info.request.id, id);
    assert_eq!(
        info.verification_problems,
        vec![VerificationProblem::IncorrectMetadata]
    );

    // Request should be accepted after verification
    let new_id = client
        .send_verification_request(&verification_request)
        .await;
    assert_eq!(new_id, 2);
    contract_verifier
        .pick_up_next_request(new_id, &verification_request, bytecode_kind)
        .await;

    // Verify new contract
    verification_info.request.id = new_id;
    verification_info.verification_problems.clear();
    contract_verifier.verify_contract(verification_info).await;

    let status = client.verification_status(new_id).await;
    assert_eq!(status.status, "successful");

    // Now verification info should be updated
    let info = client.verification_info(address).await;
    assert_eq!(info.request.id, new_id);
    assert_eq!(info.verification_problems, vec![]);
}

#[test_casing(2, [BytecodeMarker::EraVm, BytecodeMarker::Evm])]
#[tokio::test]
async fn submitting_request_with_invalid_compiler_type(bytecode_kind: BytecodeMarker) {
    let pool = ConnectionPool::test_pool().await;
    let client = MockApiClient::new(pool.clone());
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let address = Address::repeat_byte(0x23);
    mock_deploy_contract(&mut storage, address, bytecode_kind).await;

    let verification_request = serde_json::json!({
        "contractAddress": address,
        "sourceCode": "contract Test {}",
        "contractName": "Test",
        // Intentionally incorrect versions "shape"
        "compilerZksolcVersion": match bytecode_kind {
            BytecodeMarker::Evm => Some(ZKSOLC_VERSION),
            BytecodeMarker::EraVm => None,
        },
        "compilerSolcVersion": SOLC_VERSION,
        "optimizationUsed": true,
    });
    let expected_err = match bytecode_kind {
        BytecodeMarker::Evm => ApiError::BogusZkCompilerVersion,
        BytecodeMarker::EraVm => ApiError::MissingZkCompilerVersion,
    };
    client
        .assert_verification_request_error(&verification_request, expected_err)
        .await;
}

#[test_casing(2, [BytecodeMarker::EraVm, BytecodeMarker::Evm])]
#[tokio::test]
async fn submitting_request_with_unsupported_solc(bytecode_kind: BytecodeMarker) {
    let pool = ConnectionPool::test_pool().await;
    let client = MockApiClient::new(pool.clone());
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let address = Address::repeat_byte(0x23);
    mock_deploy_contract(&mut storage, address, bytecode_kind).await;

    let verification_request = serde_json::json!({
        "contractAddress": address,
        "sourceCode": "contract Test {}",
        "contractName": "Test",
        "compilerZksolcVersion": match bytecode_kind {
            BytecodeMarker::Evm => None,
            BytecodeMarker::EraVm => Some(ZKSOLC_VERSION),
        },
        "compilerSolcVersion": "1.0.0",
        "optimizationUsed": true,
    });
    client
        .assert_verification_request_error(
            &verification_request,
            ApiError::UnsupportedCompilerVersions,
        )
        .await;
}

#[tokio::test]
async fn submitting_request_with_unsupported_zksolc() {
    let pool = ConnectionPool::test_pool().await;
    let client = MockApiClient::new(pool.clone());
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let address = Address::repeat_byte(0x23);
    mock_deploy_contract(&mut storage, address, BytecodeMarker::EraVm).await;

    let verification_request = serde_json::json!({
        "contractAddress": address,
        "sourceCode": "contract Test {}",
        "contractName": "Test",
        "compilerZksolcVersion": "1000.0.0",
        "compilerSolcVersion": SOLC_VERSION,
        "optimizationUsed": true,
    });
    client
        .assert_verification_request_error(
            &verification_request,
            ApiError::UnsupportedCompilerVersions,
        )
        .await;
}

#[tokio::test]
async fn querying_missing_request() {
    let pool = ConnectionPool::test_pool().await;
    let client = MockApiClient::new(pool.clone());
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;
    client
        .assert_verification_request_status_error(1, ApiError::RequestNotFound)
        .await;
}

#[tokio::test]
async fn querying_missing_verification_info() {
    let pool = ConnectionPool::test_pool().await;
    let client = MockApiClient::new(pool.clone());
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;
    client
        .assert_verification_info_error(
            Address::repeat_byte(0x23),
            ApiError::VerificationInfoNotFound,
        )
        .await;
}

#[tokio::test]
async fn mismatched_compiler_type() {
    let pool = ConnectionPool::test_pool().await;
    let client = MockApiClient::new(pool.clone());
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;
    let address = Address::repeat_byte(0x23);
    mock_deploy_contract(&mut storage, address, BytecodeMarker::EraVm).await;

    let verification_request = serde_json::json!({
        "contractAddress": address,
        "sourceCode": "contract Test {}",
        "contractName": "Test",
        "compilerVyperVersion": "1.0.1",
        "optimizationUsed": true,
    });
    client
        .assert_verification_request_error(
            &verification_request,
            ApiError::IncorrectCompilerVersions,
        )
        .await;
}
