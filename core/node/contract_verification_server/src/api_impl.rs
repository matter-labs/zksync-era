use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use zksync_dal::{CoreDal, DalError};
use zksync_types::{
    contract_verification_api::{
        VerificationIncomingRequest, VerificationInfo, VerificationRequestStatus,
    },
    Address,
};

use super::{api_decl::RestApi, metrics::METRICS};

#[derive(Debug)]
pub(crate) enum ApiError {
    IncorrectCompilerVersions,
    NoDeployedContract,
    RequestNotFound,
    VerificationInfoNotFound,
    Internal(anyhow::Error),
}

impl From<DalError> for ApiError {
    fn from(err: DalError) -> Self {
        Self::Internal(err.generalize())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status_code, message) = match self {
            Self::IncorrectCompilerVersions => (
                StatusCode::BAD_REQUEST,
                "incorrect compiler versions".to_owned(),
            ),
            Self::NoDeployedContract => (
                StatusCode::BAD_REQUEST,
                "There is no deployed contract on this address".to_owned(),
            ),
            Self::RequestNotFound => (StatusCode::NOT_FOUND, "request not found".to_owned()),
            Self::VerificationInfoNotFound => (
                StatusCode::NOT_FOUND,
                "verification info not found for address".to_owned(),
            ),
            Self::Internal(err) => {
                // Do not expose the error details to the client, but log it.
                tracing::warn!("Internal error: {err:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_owned(),
                )
            }
        };
        (status_code, message).into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

impl RestApi {
    #[tracing::instrument(skip(query))]
    fn validate_contract_verification_query(
        query: &VerificationIncomingRequest,
    ) -> Result<(), ApiError> {
        if query.source_code_data.compiler_type() != query.compiler_versions.compiler_type() {
            return Err(ApiError::IncorrectCompilerVersions);
        }
        Ok(())
    }

    /// Add a contract verification job to the queue if the requested contract wasn't previously verified.
    #[tracing::instrument(skip(self_, request))]
    pub async fn verification(
        State(self_): State<Arc<Self>>,
        Json(request): Json<VerificationIncomingRequest>,
    ) -> ApiResult<usize> {
        let method_latency = METRICS.call[&"contract_verification"].start();
        Self::validate_contract_verification_query(&request)?;
        let mut storage = self_
            .master_connection_pool
            .connection_tagged("api")
            .await?;

        // FIXME: check bytecode kind vs versions
        if !storage
            .storage_logs_dal()
            .is_contract_deployed_at_address(request.contract_address)
            .await
        {
            return Err(ApiError::NoDeployedContract);
        }

        let request_id = storage
            .contract_verification_dal()
            .add_contract_verification_request(&request)
            .await?;
        method_latency.observe();
        Ok(Json(request_id))
    }

    #[tracing::instrument(skip(self_))]
    pub async fn verification_request_status(
        State(self_): State<Arc<Self>>,
        id: Path<usize>,
    ) -> ApiResult<VerificationRequestStatus> {
        let method_latency = METRICS.call[&"contract_verification_request_status"].start();
        let status = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await?
            .contract_verification_dal()
            .get_verification_request_status(*id)
            .await?
            .ok_or(ApiError::RequestNotFound)?;

        method_latency.observe();
        Ok(Json(status))
    }

    #[tracing::instrument(skip(self_))]
    pub async fn zksolc_versions(State(self_): State<Arc<Self>>) -> ApiResult<Vec<String>> {
        let method_latency = METRICS.call[&"contract_verification_zksolc_versions"].start();
        let versions = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await?
            .contract_verification_dal()
            .get_zksolc_versions()
            .await?;
        method_latency.observe();
        Ok(Json(versions))
    }

    #[tracing::instrument(skip(self_))]
    pub async fn solc_versions(State(self_): State<Arc<Self>>) -> ApiResult<Vec<String>> {
        let method_latency = METRICS.call[&"contract_verification_solc_versions"].start();
        let versions = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await?
            .contract_verification_dal()
            .get_solc_versions()
            .await?;
        method_latency.observe();
        Ok(Json(versions))
    }

    #[tracing::instrument(skip(self_))]
    pub async fn zkvyper_versions(State(self_): State<Arc<Self>>) -> ApiResult<Vec<String>> {
        let method_latency = METRICS.call[&"contract_verification_zkvyper_versions"].start();
        let versions = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await?
            .contract_verification_dal()
            .get_zkvyper_versions()
            .await?;
        method_latency.observe();
        Ok(Json(versions))
    }

    #[tracing::instrument(skip(self_))]
    pub async fn vyper_versions(State(self_): State<Arc<Self>>) -> ApiResult<Vec<String>> {
        let method_latency = METRICS.call[&"contract_verification_vyper_versions"].start();
        let versions = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await?
            .contract_verification_dal()
            .get_vyper_versions()
            .await?;
        method_latency.observe();
        Ok(Json(versions))
    }

    #[tracing::instrument(skip(self_))]
    pub async fn verification_info(
        State(self_): State<Arc<Self>>,
        address: Path<Address>,
    ) -> ApiResult<VerificationInfo> {
        let method_latency = METRICS.call[&"contract_verification_info"].start();
        let info = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await?
            .contract_verification_dal()
            .get_contract_verification_info(*address)
            .await?
            .ok_or(ApiError::VerificationInfoNotFound)?;
        method_latency.observe();
        Ok(Json(info))
    }
}
