use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::Response,
    Json,
};
use serde::Serialize;
use zksync_dal::CoreDal;
use zksync_types::{contract_verification_api::VerificationIncomingRequest, Address};

use super::{api_decl::RestApi, metrics::METRICS};

fn ok_json(data: impl Serialize) -> Response<String> {
    Response::builder()
        .status(axum::http::StatusCode::OK)
        .body(serde_json::to_string(&data).expect("Failed to serialize"))
        .unwrap()
}

fn bad_request(message: &str) -> Response<String> {
    Response::builder()
        .status(axum::http::StatusCode::BAD_REQUEST)
        .body(message.to_string())
        .unwrap()
}

fn not_found() -> Response<String> {
    Response::builder()
        .status(axum::http::StatusCode::NOT_FOUND)
        .body(String::new())
        .unwrap()
}

impl RestApi {
    #[tracing::instrument(skip(query))]
    fn validate_contract_verification_query(
        query: &VerificationIncomingRequest,
    ) -> Result<(), Response<String>> {
        if query.source_code_data.compiler_type() != query.compiler_versions.compiler_type() {
            return Err(bad_request("incorrect compiler versions"));
        }

        Ok(())
    }

    /// Add a contract verification job to the queue if the requested contract wasn't previously verified.
    #[tracing::instrument(skip(self_, request))]
    pub async fn verification(
        State(self_): State<Arc<Self>>,
        Json(request): Json<VerificationIncomingRequest>,
    ) -> Response<String> {
        let method_latency = METRICS.call[&"contract_verification"].start();
        if let Err(res) = Self::validate_contract_verification_query(&request) {
            return res;
        }
        let mut storage = self_
            .master_connection_pool
            .connection_tagged("api")
            .await
            .unwrap();

        if !storage
            .storage_logs_dal()
            .is_contract_deployed_at_address(request.contract_address)
            .await
        {
            return bad_request("There is no deployed contract on this address");
        }

        let request_id = storage
            .contract_verification_dal()
            .add_contract_verification_request(request)
            .await
            .unwrap();

        method_latency.observe();
        ok_json(request_id)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn verification_request_status(
        State(self_): State<Arc<Self>>,
        id: Path<usize>,
    ) -> Response<String> {
        let method_latency = METRICS.call[&"contract_verification_request_status"].start();
        let status = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await
            .unwrap()
            .contract_verification_dal()
            .get_verification_request_status(*id)
            .await
            .unwrap();

        method_latency.observe();
        match status {
            Some(status) => ok_json(status),
            None => not_found(),
        }
    }

    #[tracing::instrument(skip(self_))]
    pub async fn zksolc_versions(State(self_): State<Arc<Self>>) -> Response<String> {
        let method_latency = METRICS.call[&"contract_verification_zksolc_versions"].start();
        let versions = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await
            .unwrap()
            .contract_verification_dal()
            .get_zksolc_versions()
            .await
            .unwrap();

        method_latency.observe();
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn solc_versions(State(self_): State<Arc<Self>>) -> Response<String> {
        let method_latency = METRICS.call[&"contract_verification_solc_versions"].start();
        let versions = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await
            .unwrap()
            .contract_verification_dal()
            .get_solc_versions()
            .await
            .unwrap();

        method_latency.observe();
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn zkvyper_versions(State(self_): State<Arc<Self>>) -> Response<String> {
        let method_latency = METRICS.call[&"contract_verification_zkvyper_versions"].start();
        let versions = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await
            .unwrap()
            .contract_verification_dal()
            .get_zkvyper_versions()
            .await
            .unwrap();

        method_latency.observe();
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn vyper_versions(State(self_): State<Arc<Self>>) -> Response<String> {
        let method_latency = METRICS.call[&"contract_verification_vyper_versions"].start();
        let versions = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await
            .unwrap()
            .contract_verification_dal()
            .get_vyper_versions()
            .await
            .unwrap();

        method_latency.observe();
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn verification_info(
        State(self_): State<Arc<Self>>,
        address: Path<Address>,
    ) -> Response<String> {
        let method_latency = METRICS.call[&"contract_verification_info"].start();

        let info = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await
            .unwrap()
            .contract_verification_dal()
            .get_contract_verification_info(*address)
            .await
            .unwrap();

        method_latency.observe();
        match info {
            Some(info) => ok_json(info),
            None => not_found(),
        }
    }
}
