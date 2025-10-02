use std::{str, time::Duration, vec};

use axum::{
    body::Body,
    http::{header, Method, Request, Response, StatusCode},
    response::IntoResponse,
};
use http_body_util::BodyExt as _;
use serde::Deserialize;
use tower::ServiceExt;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_node_test_utils::create_l2_block;
use zksync_types::{
    bytecode::{BytecodeHash, BytecodeMarker},
    contract_verification::{
        api::{
            CompilationArtifacts, CompilerVersions, VerificationIncomingRequest, VerificationInfo,
            VerificationRequest, VerificationRequestStatus,
        },
        etherscan::EtherscanResponse,
    },
    get_code_key, Address, L2BlockNumber, ProtocolVersion, StorageLog, H256,
};

use crate::{api_impl::ApiError, RestApi};

pub(super) const SOLC_VERSION: &str = "0.8.27";
pub(super) const ZKSOLC_VERSION: &str = "1.5.6";

pub(super) async fn prepare_storage(storage: &mut Connection<'_, Core>) {
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

pub(super) async fn mock_deploy_contract(
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

pub(super) fn mock_verification_info(
    id: usize,
    verification_request: &serde_json::Value,
    abi: Option<serde_json::Value>,
) -> VerificationInfo {
    VerificationInfo {
        request: VerificationRequest {
            id,
            req: serde_json::from_value(verification_request.clone()).unwrap(),
        },
        artifacts: CompilationArtifacts {
            bytecode: vec![0xff, 32],
            deployed_bytecode: None,
            abi: abi.unwrap_or_default(),
            immutable_refs: Default::default(),
        },
        verified_at: Default::default(),
        verification_problems: Vec::new(),
    }
}

#[derive(Debug)]
pub(super) struct MockContractVerifier {
    pool: ConnectionPool<Core>,
}

impl MockContractVerifier {
    pub fn new(pool: ConnectionPool<Core>) -> Self {
        Self { pool }
    }

    pub async fn pick_up_next_request(
        &self,
        id: usize,
        initial_request: &serde_json::Value,
        bytecode_kind: BytecodeMarker,
    ) {
        let initial_request: VerificationIncomingRequest =
            serde_json::from_value(initial_request.clone()).expect("Invalid request");
        let mut storage = self.pool.connection().await.unwrap();
        let request = storage
            .contract_verification_dal()
            .get_next_queued_verification_request(Duration::from_secs(600))
            .await
            .unwrap()
            .expect("request not persisted");

        assert_eq!(request.id, id);
        assert_eq!(
            request.req.contract_address,
            initial_request.contract_address
        );
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
        assert_eq!(request.req.contract_name, initial_request.contract_name);
        assert_eq!(
            request.req.optimization_used,
            initial_request.optimization_used
        );
    }

    pub async fn verify_contract(&self, verification_info: VerificationInfo) {
        // Doesn't matter for simple cases. To prevent boilerplate, if these fields would be
        // required, it's better to add a new method.
        let bytecode_keccak256 = H256::repeat_byte(0x11);
        let bytecode_without_metadata_keccak256 = H256::repeat_byte(0x22);

        let mut storage = self.pool.connection().await.unwrap();
        storage
            .contract_verification_dal()
            .save_verification_info(
                verification_info,
                bytecode_keccak256,
                bytecode_without_metadata_keccak256,
            )
            .await
            .unwrap();
    }
}

#[derive(Debug)]
pub(super) struct MockApiClient {
    router: axum::Router,
}

impl MockApiClient {
    pub fn new(pool: ConnectionPool<Core>) -> Self {
        Self {
            router: RestApi::new(pool.clone(), pool).into_router(),
        }
    }

    pub async fn send_verification_request(&self, request: &serde_json::Value) -> usize {
        let response = self
            .send_request("/contract_verification", Some(request))
            .await;
        Self::json_response::<usize>(response).await
    }

    pub async fn send_etherscan_post_request<T>(&self, request: &T) -> EtherscanResponse
    where
        T: serde::Serialize,
    {
        let response = self.send_form("/contract_verification", request).await;
        Self::json_response::<EtherscanResponse>(response).await
    }

    pub async fn etherscan_get_verification_status(&self, id: usize) -> EtherscanResponse {
        let response = self
            .send_request(
                &format!(
                    "/contract_verification?module=contract&action=checkverifystatus&guid={id}"
                ),
                None,
            )
            .await;
        Self::json_response::<EtherscanResponse>(response).await
    }

    pub async fn etherscan_get_source_code(&self, address: Address) -> EtherscanResponse {
        let response = self
            .send_request(
                &format!(
                    "/contract_verification?module=contract&action=getsourcecode&address={:#?}",
                    address
                ),
                None,
            )
            .await;
        Self::json_response::<EtherscanResponse>(response).await
    }

    pub async fn etherscan_get_abi(&self, address: Address) -> EtherscanResponse {
        let response = self
            .send_request(
                &format!(
                    "/contract_verification?module=contract&action=getabi&address={:#?}",
                    address
                ),
                None,
            )
            .await;
        Self::json_response::<EtherscanResponse>(response).await
    }

    pub async fn assert_verification_request_error(
        &self,
        request: &serde_json::Value,
        expected_err: ApiError,
    ) {
        let response = self
            .send_request("/contract_verification", Some(request))
            .await;
        Self::assert_response_error(response, expected_err).await;
    }

    pub async fn assert_verification_request_status_error(
        &self,
        id: usize,
        expected_err: ApiError,
    ) {
        let response = self
            .send_request(&format!("/contract_verification/{id}"), None)
            .await;
        Self::assert_response_error(response, expected_err).await;
    }

    pub async fn verification_status(&self, id: usize) -> VerificationRequestStatus {
        let response = self
            .send_request(&format!("/contract_verification/{id}"), None)
            .await;
        Self::json_response::<VerificationRequestStatus>(response).await
    }

    pub async fn verification_info(&self, address: Address) -> VerificationInfo {
        let response = self
            .send_request(&format!("/contract_verification/info/{address:?}"), None)
            .await;
        Self::json_response::<VerificationInfo>(response).await
    }

    pub async fn assert_verification_info_error(&self, address: Address, expected_err: ApiError) {
        let response = self
            .send_request(&format!("/contract_verification/info/{address:?}"), None)
            .await;
        Self::assert_response_error(response, expected_err).await;
    }

    pub async fn zksolc_versions(&self) -> Vec<String> {
        let response = self
            .send_request("/contract_verification/zksolc_versions", None)
            .await;
        Self::json_response::<Vec<String>>(response).await
    }

    pub async fn solc_versions(&self) -> Vec<String> {
        let response = self
            .send_request("/contract_verification/solc_versions", None)
            .await;
        Self::json_response::<Vec<String>>(response).await
    }

    async fn send_request(&self, url: &str, body: Option<&serde_json::Value>) -> Response<Body> {
        let (method, body) = match body {
            Some(body) => (Method::POST, Body::from(serde_json::to_vec(body).unwrap())),
            None => (Method::GET, Body::empty()),
        };

        let req = Request::builder()
            .method(method)
            .uri(url)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .unwrap();

        self.router.clone().oneshot(req).await.unwrap()
    }

    async fn send_form<T>(&self, url: &str, body: &T) -> Response<Body>
    where
        T: serde::Serialize,
    {
        let form = serde_urlencoded::to_string(body).expect("Unable to serialize body into form");
        let req = Request::builder()
            .method(Method::POST)
            .uri(url)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form)
            .unwrap();

        self.router.clone().oneshot(req).await.unwrap()
    }

    async fn json_response<T: for<'a> Deserialize<'a>>(response: Response<Body>) -> T {
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let response = response.into_body();
        let response = response.collect().await.unwrap().to_bytes();
        serde_json::from_slice(&response).expect("Unable to deserialize response")
    }

    async fn assert_response_error(response: Response<Body>, expected_err: ApiError) {
        let expected_message = expected_err.message();
        let expected_status = expected_err.into_response().status();

        let error_status = response.status();
        let error_message = response.collect().await.unwrap().to_bytes();
        let error_message = str::from_utf8(&error_message).unwrap();
        assert_eq!(error_message, expected_message);
        assert_eq!(error_status, expected_status, "Message: {error_message}");
    }
}
