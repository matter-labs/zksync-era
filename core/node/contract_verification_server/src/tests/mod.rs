//! Tests for contract verification API server.

use std::{str, vec};

use test_casing::test_casing;
use utils::{mock_verification_info, MockApiClient, MockContractVerifier};
use zksync_types::{
    bytecode::BytecodeMarker,
    contract_verification::{
        api::{
            CompilationArtifacts, CompilerVersions, SourceCodeData, VerificationIncomingRequest,
            VerificationInfo, VerificationProblem, VerificationRequest,
        },
        contract_identifier::{ContractIdentifier, DetectedMetadata},
        etherscan::{
            EtherscanBoolean, EtherscanCodeFormat, EtherscanPostPayload, EtherscanPostRequest,
            EtherscanResult, EtherscanSourceCodeResponse, EtherscanVerificationRequest,
        },
    },
    Address,
};

use super::*;
use crate::{
    api_impl::ApiError,
    tests::utils::{
        mock_deploy_contract, mock_deploy_contract_with_bytecode, mock_deploy_evm_contract,
        prepare_storage, store_verification_info, SOLC_VERSION, ZKSOLC_VERSION,
    },
};

/// EraVM bytecode of a `value() -> 1` contract compiled with metadata disabled (`appendCBOR:false`).
/// Its trailing word is functional code (read as `code[12]`), yet the keccak heuristic strips it.
const METADATA_DISABLED_BYTECODE: &str = "0000008003000039000000400030043f0000000100200190000000110000c13d0000000900100198000000190000613d000000000101043b0000000a011001970000000b0010009c000000190000c13d0000000001000416000000000001004b000000190000c13d0000000101000039000000800010043f0000000c010000410000001c0001042e0000000001000416000000000001004b000000190000c13d00000020010000390000010000100443000001200000044300000008010000410000001c0001042e00000000010000190000001d000104300000001b000004320000001c0001042e0000001d0001043000000000000000000000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000fffffffc000000000000000000000000ffffffff000000000000000000000000000000000000000000000000000000003fa4f245000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000800000000000000000";

fn eravm_verification_info(
    address: Address,
    source_code_data: SourceCodeData,
    zksolc_version: Option<&str>,
) -> VerificationInfo {
    VerificationInfo {
        request: VerificationRequest {
            id: 1,
            req: VerificationIncomingRequest {
                contract_address: address,
                source_code_data,
                contract_name: "Counter".to_owned(),
                compiler_versions: CompilerVersions::Solc {
                    compiler_zksolc_version: zksolc_version.map(str::to_owned),
                    compiler_solc_version: SOLC_VERSION.to_owned(),
                },
                optimization_used: true,
                optimizer_mode: None,
                constructor_arguments: Default::default(),
                is_system: false,
                force_evmla: false,
                evm_specific: Default::default(),
            },
        },
        artifacts: CompilationArtifacts {
            bytecode: vec![],
            deployed_bytecode: None,
            abi: serde_json::json!([]),
            immutable_refs: Default::default(),
        },
        verified_at: Default::default(),
        verification_problems: Vec::new(),
    }
}

/// Standard-JSON source that disables metadata (`bytecodeHash:"none"`, `appendCBOR:false`), under
/// which the trailing EraVM word is functional code rather than a metadata hash.
fn metadata_disabled_standard_json() -> SourceCodeData {
    SourceCodeData::StandardJsonInput(
        serde_json::json!({
            "language": "Solidity",
            "sources": { "Counter.sol": { "content": "contract Counter {}" } },
            "settings": {
                "metadata": { "bytecodeHash": "none", "appendCBOR": false },
                "optimizer": { "enabled": true }
            }
        })
        .as_object()
        .unwrap()
        .clone(),
    )
}

/// Bytecode colliding with `METADATA_DISABLED_BYTECODE` on the metadata-stripped hash but differing
/// in the trailing functional word. When `cbor_cloaked`, an empty CBOR map makes it CBOR-detected
/// rather than keccak-detected (which a deployed-side-only guard would miss).
fn divergent_bytecode(original: &[u8], cbor_cloaked: bool) -> Vec<u8> {
    let mut deployed = original[..original.len() - 32].to_vec();
    if cbor_cloaked {
        deployed.extend_from_slice(&[0xDE; 29]);
        deployed.extend_from_slice(&[0xA0, 0x00, 0x01]);
    } else {
        deployed.extend_from_slice(&[0; 32]);
    }
    deployed
}

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
    let verification_info = mock_verification_info(id, &verification_request, None);
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
async fn submitting_etherscan_requests(bytecode_kind: BytecodeMarker) {
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
    let abi_json = serde_json::json!([
            {
                "inputs": [],
                "stateMutability": "nonpayable",
                "type": "constructor"
            }]
    );
    let verification_json = serde_json::to_value(
        etherscan_verification_req
            .clone()
            .to_verification_request()
            .unwrap(),
    )
    .unwrap();

    let verification_request = EtherscanPostRequest {
        module: "contract".to_string(),
        payload: EtherscanPostPayload::VerifySourceCode(etherscan_verification_req),
    };

    mock_deploy_contract(&mut storage, address, bytecode_kind).await;

    let etherscan_response = client
        .send_etherscan_post_request(&verification_request)
        .await;

    assert_eq!(etherscan_response.status, "1");
    assert_eq!(etherscan_response.message, "OK");
    assert_eq!(
        etherscan_response.result,
        EtherscanResult::String("1".to_string())
    );

    let id = 1_usize;

    // Duplicate request should not be created.
    let etherscan_response = client
        .send_etherscan_post_request(&verification_request)
        .await;

    assert_eq!(etherscan_response.status, "0");
    assert_eq!(etherscan_response.message, "NOTOK");
    assert_eq!(
        etherscan_response.result,
        EtherscanResult::String(
            "active request for this contract already exists, ID: 1".to_string()
        )
    );

    // Pick up the request.
    contract_verifier
        .pick_up_next_request(id, &verification_json, bytecode_kind)
        .await;

    // Should be in progress now.
    let check_status_request = EtherscanPostRequest {
        module: "contract".to_string(),
        payload: EtherscanPostPayload::CheckVerifyStatus {
            guid: id.to_string(),
        },
    };

    // check verify status with POST request
    let etherscan_response = client
        .send_etherscan_post_request(&check_status_request)
        .await;
    assert_eq!(etherscan_response.status, "0");
    assert_eq!(etherscan_response.message, "NOTOK");
    assert_eq!(
        etherscan_response.result,
        EtherscanResult::String("Pending in queue".to_string())
    );

    // check checkverifystatus with GET request
    let etherscan_response = client.etherscan_get_verification_status(id).await;
    assert_eq!(etherscan_response.status, "0");
    assert_eq!(etherscan_response.message, "NOTOK");
    assert_eq!(
        etherscan_response.result,
        EtherscanResult::String("Pending in queue".to_string())
    );

    // Verify contract
    let verification_info = mock_verification_info(id, &verification_json, Some(abi_json));
    contract_verifier.verify_contract(verification_info).await;

    let etherscan_response = client
        .send_etherscan_post_request(&check_status_request)
        .await;
    assert_eq!(etherscan_response.status, "1");
    assert_eq!(etherscan_response.message, "OK");
    assert_eq!(
        etherscan_response.result,
        EtherscanResult::String("Pass - Verified".to_string())
    );

    // We should be able to fetch verification info
    let info = client.verification_info(address).await;
    assert_eq!(info.request.id, id);
    assert_eq!(info.artifacts.bytecode, vec![0xff, 32]);

    let contract_source_code = client.etherscan_get_source_code(address).await;
    assert_eq!(contract_source_code.status, "1");
    assert_eq!(contract_source_code.message, "OK");
    assert_eq!(
        contract_source_code.result,
        EtherscanResult::SourceCode(EtherscanSourceCodeResponse {
            source_code:
                "{\"codeFormat\":\"solidity-single-file\",\"sourceCode\":\"contract Test {}\"}"
                    .to_string(),
            abi: "[{\"inputs\":[],\"stateMutability\":\"nonpayable\",\"type\":\"constructor\"}]"
                .to_string(),
            contract_name: "Test".to_string(),
            zk_solc_version: match bytecode_kind {
                BytecodeMarker::EraVm => ZKSOLC_VERSION.to_string(),
                BytecodeMarker::Evm => Default::default(),
            },
            compiler_version: SOLC_VERSION.to_string(),
            compiler_type: "solc".to_string(),
            optimization_used: EtherscanBoolean::True,
            runs: Default::default(),
            constructor_arguments: Default::default(),
            evm_version: Default::default(),
            library: Default::default(),
            license_type: Default::default(),
            proxy: EtherscanBoolean::False,
            implementation: Default::default(),
            swarm_source: Default::default(),
            similar_match: Default::default(),
        })
    );

    let contract_source_code = client.etherscan_get_abi(address).await;
    assert_eq!(contract_source_code.status, "1");
    assert_eq!(contract_source_code.message, "OK");
    assert_eq!(
        contract_source_code.result,
        EtherscanResult::String(
            "[{\"inputs\":[],\"stateMutability\":\"nonpayable\",\"type\":\"constructor\"}]"
                .to_string()
        )
    );

    // No requests should be accepted after verification
    let etherscan_response = client
        .send_etherscan_post_request(&verification_request)
        .await;
    assert_eq!(etherscan_response.status, "0");
    assert_eq!(etherscan_response.message, "NOTOK");
    assert_eq!(
        etherscan_response.result,
        EtherscanResult::String("Contract source code already verified".to_string())
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
    let mut verification_info = mock_verification_info(id, &verification_request, None);
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

/// The similar-match fallback must NOT serve a metadata-disabled contract's source for a different
/// address whose deployed bytecode differs only in the trailing (functional) word — including when it
/// is crafted to be CBOR-detected rather than keccak-detected.
#[test_casing(2, [false, true])]
#[tokio::test]
async fn fallback_partial_match_suppressed_for_metadata_disabled_contract(cbor_cloaked: bool) {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let client = MockApiClient::new(pool.clone());
    prepare_storage(&mut storage).await;

    let original = hex::decode(METADATA_DISABLED_BYTECODE).unwrap();
    let original_id = ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, &original);
    assert_eq!(
        original_id.detected_metadata,
        Some(DetectedMetadata::Keccak256)
    );

    // An honestly verified, metadata-disabled contract B stored at its own address.
    let stored_address = Address::repeat_byte(0xb0);
    let metadata_disabled = metadata_disabled_standard_json();
    assert!(metadata_disabled.appended_metadata_disabled(Some("1.5.14")));
    store_verification_info(
        &mut storage,
        eravm_verification_info(stored_address, metadata_disabled, Some("1.5.14")),
        original_id.bytecode_keccak256,
        original_id.bytecode_without_metadata_keccak256,
    )
    .await;

    // The attacker deploys, at a different address, bytecode that collides with B on the
    // metadata-stripped hash but diverges in the trailing functional word.
    let deployed = divergent_bytecode(&original, cbor_cloaked);
    let deployed_id = ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, &deployed);
    assert_eq!(
        deployed_id.bytecode_without_metadata_keccak256,
        original_id.bytecode_without_metadata_keccak256,
        "deployed bytecode must collide on the metadata-stripped hash"
    );
    assert_ne!(
        deployed_id.bytecode_keccak256,
        original_id.bytecode_keccak256
    );
    let expected_detection = if cbor_cloaked {
        // The CBOR-cloaked variant must actually be CBOR-detected, else it would not exercise the
        // bypass of a deployed-side keccak-heuristic check.
        assert!(matches!(
            deployed_id.detected_metadata,
            Some(DetectedMetadata::Cbor { .. })
        ));
        true
    } else {
        assert_eq!(
            deployed_id.detected_metadata,
            Some(DetectedMetadata::Keccak256)
        );
        false
    };
    assert_eq!(expected_detection, cbor_cloaked);

    let attacker_address = Address::repeat_byte(0xa0);
    mock_deploy_contract_with_bytecode(&mut storage, attacker_address, deployed).await;

    // The fallback must not serve B's source for the attacker's address.
    client
        .assert_verification_info_error(attacker_address, ApiError::VerificationInfoNotFound)
        .await;
}

/// Counterpart to [`fallback_partial_match_suppressed_for_metadata_disabled_contract`]: when the
/// stored contract carries real metadata, the fallback must still serve its source for a bytecode
/// differing only in the (genuine) metadata region (no over-rejection).
#[tokio::test]
async fn fallback_partial_match_served_for_metadata_enabled_contract() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let client = MockApiClient::new(pool.clone());
    prepare_storage(&mut storage).await;

    let original = hex::decode(METADATA_DISABLED_BYTECODE).unwrap();
    let original_id = ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, &original);

    // Stored contract B is verified *without* disabling metadata.
    let stored_address = Address::repeat_byte(0xb0);
    let metadata_enabled = SourceCodeData::SolSingleFile("contract Counter {}".to_owned());
    assert!(!metadata_enabled.appended_metadata_disabled(Some("1.5.14")));
    store_verification_info(
        &mut storage,
        eravm_verification_info(stored_address, metadata_enabled, Some("1.5.14")),
        original_id.bytecode_keccak256,
        original_id.bytecode_without_metadata_keccak256,
    )
    .await;

    let attacker_address = Address::repeat_byte(0xa0);
    mock_deploy_contract_with_bytecode(
        &mut storage,
        attacker_address,
        divergent_bytecode(&original, false),
    )
    .await;

    // The fallback serves B's source (as a partial match) for the queried address.
    let info = client.verification_info(attacker_address).await;
    assert_eq!(info.request.req.contract_address, stored_address);
    assert_eq!(
        info.verification_problems,
        vec![VerificationProblem::IncorrectMetadata]
    );
}

/// Cross-marker variant: EVM-marked bytecode can collide with a metadata-disabled EraVM contract on
/// the metadata-stripped hash. Since suppression keys on the stored contract, not the queried marker,
/// this is still treated as not verified (gating on the deployed marker would let it through).
#[tokio::test]
async fn fallback_partial_match_suppressed_across_bytecode_markers() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let client = MockApiClient::new(pool.clone());
    prepare_storage(&mut storage).await;

    let original = hex::decode(METADATA_DISABLED_BYTECODE).unwrap();
    let original_id = ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, &original);

    // Store the metadata-disabled EraVM contract B.
    let stored_address = Address::repeat_byte(0xb0);
    store_verification_info(
        &mut storage,
        eravm_verification_info(
            stored_address,
            metadata_disabled_standard_json(),
            Some("1.5.14"),
        ),
        original_id.bytecode_keccak256,
        original_id.bytecode_without_metadata_keccak256,
    )
    .await;

    // The attacker deploys EVM bytecode = B's prefix followed by an empty CBOR map, so it is
    // CBOR-detected (EVM marker) yet collides with B on the metadata-stripped hash.
    let mut evm_runtime = original[..original.len() - 32].to_vec();
    evm_runtime.extend_from_slice(&[0xA0, 0x00, 0x01]);
    let evm_id = ContractIdentifier::from_bytecode(BytecodeMarker::Evm, &evm_runtime);
    assert!(matches!(
        evm_id.detected_metadata,
        Some(DetectedMetadata::Cbor { .. })
    ));
    assert_eq!(
        evm_id.bytecode_without_metadata_keccak256, original_id.bytecode_without_metadata_keccak256,
        "EVM-marked bytecode must collide on the metadata-stripped hash"
    );
    assert_ne!(evm_id.bytecode_keccak256, original_id.bytecode_keccak256);

    let attacker_address = Address::repeat_byte(0xa0);
    mock_deploy_evm_contract(&mut storage, attacker_address, evm_runtime).await;

    client
        .assert_verification_info_error(attacker_address, ApiError::VerificationInfoNotFound)
        .await;
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
