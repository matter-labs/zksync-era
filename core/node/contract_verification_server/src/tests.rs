//! Tests for contract verification API server.

use std::{str, time::Duration};

use axum::{
    body::Body,
    http::{header, Method, Request, Response, StatusCode},
};
use http_body_util::BodyExt as _;
use test_casing::test_casing;
use tower::ServiceExt;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_node_test_utils::create_l2_block;
use zksync_types::{
    bytecode::{BytecodeHash, BytecodeMarker},
    contract_verification::api::CompilerVersions,
    get_code_key, Address, L2BlockNumber, ProtocolVersion, StorageLog,
};

use super::*;
use crate::api_impl::ApiError;

const SOLC_VERSION: &str = "0.8.27";
const ZKSOLC_VERSION: &str = "1.5.6";

async fn prepare_storage(storage: &mut Connection<'_, Core>) {
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
    storage
        .blocks_dal()
        .insert_l2_block(&create_l2_block(0))
        .await
        .unwrap();

    storage
        .contract_verification_dal()
        .set_solc_versions(&[SOLC_VERSION.to_owned()])
        .await
        .unwrap();
    storage
        .contract_verification_dal()
        .set_zksolc_versions(&[ZKSOLC_VERSION.to_owned()])
        .await
        .unwrap();
}

async fn mock_deploy_contract(
    storage: &mut Connection<'_, Core>,
    address: Address,
    kind: BytecodeMarker,
) {
    let bytecode_hash = match kind {
        BytecodeMarker::EraVm => BytecodeHash::for_bytecode(&[0; 32]).value(),
        BytecodeMarker::Evm => BytecodeHash::for_evm_bytecode(0, &[0; 96]).value(),
    };
    let deploy_log = StorageLog::new_write_log(get_code_key(&address), bytecode_hash);
    storage
        .storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &[deploy_log])
        .await
        .unwrap()
}

fn post_request(body: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/contract_verification")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

async fn json_response(response: Response<Body>) -> serde_json::Value {
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    let response = response.into_body();
    let response = response.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&response).unwrap()
}

#[tokio::test]
async fn getting_compiler_versions() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let router = RestApi::new(pool.clone(), pool).into_router();
    let req = Request::builder()
        .method(Method::GET)
        .uri("/contract_verification/zksolc_versions")
        .body(Body::empty())
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    let versions = json_response(response).await;
    assert_eq!(versions, serde_json::json!([ZKSOLC_VERSION]));

    let req = Request::builder()
        .method(Method::GET)
        .uri("/contract_verification/solc_versions")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(req).await.unwrap();
    let versions = json_response(response).await;
    assert_eq!(versions, serde_json::json!([SOLC_VERSION]));
}

#[test_casing(2, [BytecodeMarker::EraVm, BytecodeMarker::Evm])]
#[tokio::test]
async fn submitting_request(bytecode_kind: BytecodeMarker) {
    let pool = ConnectionPool::test_pool().await;
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

    let router = RestApi::new(pool.clone(), pool).into_router();
    let response = router
        .clone()
        .oneshot(post_request(&verification_request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST); // the address is not deployed to
    let error_message = response.collect().await.unwrap().to_bytes();
    let error_message = str::from_utf8(&error_message).unwrap();
    assert_eq!(error_message, ApiError::NoDeployedContract.message());

    mock_deploy_contract(&mut storage, address, bytecode_kind).await;

    let response = router
        .clone()
        .oneshot(post_request(&verification_request))
        .await
        .unwrap();
    let id = json_response(response).await;
    assert_eq!(id, serde_json::json!(1));

    let request = storage
        .contract_verification_dal()
        .get_next_queued_verification_request(Duration::from_secs(600))
        .await
        .unwrap()
        .expect("request not persisted");
    assert_eq!(request.id, 1);
    assert_eq!(request.req.contract_address, address);
    assert_eq!(
        request.req.compiler_versions,
        CompilerVersions::Solc {
            compiler_zksolc_version: match bytecode_kind {
                BytecodeMarker::EraVm => Some(ZKSOLC_VERSION.to_owned()),
                BytecodeMarker::Evm => None,
            },
            compiler_solc_version: SOLC_VERSION.to_owned(),
        }
    );
    assert_eq!(request.req.contract_name, "Test");
    assert!(request.req.optimization_used);

    let req = Request::builder()
        .method(Method::GET)
        .uri("/contract_verification/1")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(req).await.unwrap();
    let request_status = json_response(response).await;
    assert_eq!(request_status["status"], "in_progress");
}

#[test_casing(2, [BytecodeMarker::EraVm, BytecodeMarker::Evm])]
#[tokio::test]
async fn submitting_request_with_invalid_compiler_type(bytecode_kind: BytecodeMarker) {
    let pool = ConnectionPool::test_pool().await;
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
    let router = RestApi::new(pool.clone(), pool).into_router();
    let response = router
        .oneshot(post_request(&verification_request))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error_message = response.collect().await.unwrap().to_bytes();
    let error_message = str::from_utf8(&error_message).unwrap();
    let expected_message = match bytecode_kind {
        BytecodeMarker::Evm => ApiError::BogusZkCompilerVersion.message(),
        BytecodeMarker::EraVm => ApiError::MissingZkCompilerVersion.message(),
    };
    assert_eq!(error_message, expected_message);
}

#[test_casing(2, [BytecodeMarker::EraVm, BytecodeMarker::Evm])]
#[tokio::test]
async fn submitting_request_with_unsupported_solc(bytecode_kind: BytecodeMarker) {
    let pool = ConnectionPool::test_pool().await;
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
    let router = RestApi::new(pool.clone(), pool).into_router();
    let response = router
        .oneshot(post_request(&verification_request))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error_message = response.collect().await.unwrap().to_bytes();
    let error_message = str::from_utf8(&error_message).unwrap();
    assert_eq!(
        error_message,
        ApiError::UnsupportedCompilerVersions.message()
    );
}

#[tokio::test]
async fn submitting_request_with_unsupported_zksolc() {
    let pool = ConnectionPool::test_pool().await;
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
    let router = RestApi::new(pool.clone(), pool).into_router();
    let response = router
        .oneshot(post_request(&verification_request))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error_message = response.collect().await.unwrap().to_bytes();
    let error_message = str::from_utf8(&error_message).unwrap();
    assert_eq!(
        error_message,
        ApiError::UnsupportedCompilerVersions.message()
    );
}

#[tokio::test]
async fn querying_missing_request() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;
    let router = RestApi::new(pool.clone(), pool).into_router();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/contract_verification/1")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let error_message = response.collect().await.unwrap().to_bytes();
    let error_message = str::from_utf8(&error_message).unwrap();
    assert_eq!(error_message, ApiError::RequestNotFound.message());
}

#[tokio::test]
async fn querying_missing_verification_info() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;
    let router = RestApi::new(pool.clone(), pool).into_router();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/contract_verification/info/0x2323232323232323232323232323232323232323")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let error_message = response.collect().await.unwrap().to_bytes();
    let error_message = str::from_utf8(&error_message).unwrap();
    assert_eq!(error_message, ApiError::VerificationInfoNotFound.message());
}

#[tokio::test]
async fn mismatched_compiler_type() {
    let pool = ConnectionPool::test_pool().await;
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

    let router = RestApi::new(pool.clone(), pool).into_router();
    let response = router
        .oneshot(post_request(&verification_request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error_message = response.collect().await.unwrap().to_bytes();
    let error_message = str::from_utf8(&error_message).unwrap();
    assert_eq!(error_message, ApiError::IncorrectCompilerVersions.message());
}
