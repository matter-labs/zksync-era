use std::{collections::HashSet, iter, sync::Arc};

use anyhow::Context as _;
use axum::{
    body::{to_bytes, Body},
    extract::{Path, Query, State},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use zksync_dal::{contract_verification_dal::ContractVerificationDal, CoreDal, DalError};
use zksync_types::{
    bytecode::{trim_bytecode, BytecodeHash, BytecodeMarker},
    contract_verification::{
        api::{
            CompilerVersions, SourceCodeData, VerificationIncomingRequest, VerificationInfo,
            VerificationProblem, VerificationRequestStatus,
        },
        contract_identifier::ContractIdentifier,
        etherscan::{
            EtherscanGetParams, EtherscanGetPayload, EtherscanPostPayload, EtherscanPostRequest,
            EtherscanResponse, EtherscanResult,
        },
    },
    Address,
};

use super::{api_decl::RestApi, metrics::METRICS};

#[allow(clippy::large_enum_variant)]
#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum APIPostResponse {
    EtherscanResponse(EtherscanResponse),
    VerificationId(usize),
}

#[derive(Debug)]
pub(crate) enum ApiError {
    IncorrectCompilerVersions,
    UnsupportedCompilerVersions,
    MissingZkCompilerVersion,
    BogusZkCompilerVersion,
    NoDeployedContract,
    RequestNotFound,
    VerificationInfoNotFound,
    AlreadyVerified,
    ActiveRequestExists(usize),
    Internal(anyhow::Error),
    DeserializationError(anyhow::Error),
    UnsupportedContentType,
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

impl From<DalError> for ApiError {
    fn from(err: DalError) -> Self {
        Self::Internal(err.generalize())
    }
}

impl ApiError {
    pub fn message(&self) -> String {
        match self {
            Self::IncorrectCompilerVersions => "incorrect compiler versions".into(),
            Self::UnsupportedCompilerVersions => "unsupported compiler versions".into(),
            Self::MissingZkCompilerVersion => {
                "missing zk compiler version for EraVM bytecode".into()
            }
            Self::BogusZkCompilerVersion => "zk compiler version specified for EVM bytecode".into(),
            Self::NoDeployedContract => "There is no deployed contract on this address".into(),
            Self::RequestNotFound => "request not found".into(),
            Self::VerificationInfoNotFound => "verification info not found for address".into(),
            Self::AlreadyVerified => "contract is already verified".into(),
            Self::ActiveRequestExists(id) => {
                format!("active request for this contract already exists, ID: {id}")
            }
            Self::Internal(_) => "internal server error".into(),
            Self::UnsupportedContentType => "Specified content type is not supported".into(),
            Self::DeserializationError(e) => format!("Failed to deserialize the request: {}", e),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status_code = match &self {
            Self::IncorrectCompilerVersions
            | Self::UnsupportedCompilerVersions
            | Self::MissingZkCompilerVersion
            | Self::BogusZkCompilerVersion
            | Self::NoDeployedContract
            | Self::AlreadyVerified
            | Self::ActiveRequestExists(_)
            | Self::DeserializationError(_) => StatusCode::BAD_REQUEST,

            Self::UnsupportedContentType => StatusCode::UNSUPPORTED_MEDIA_TYPE,

            Self::RequestNotFound | Self::VerificationInfoNotFound => StatusCode::NOT_FOUND,

            Self::Internal(err) => {
                // Do not expose the error details to the client, but log it.
                tracing::warn!("Internal error: {err:#}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        (status_code, self.message()).into_response()
    }
}

impl From<ApiError> for EtherscanResponse {
    fn from(api_error: ApiError) -> Self {
        match api_error {
            ApiError::AlreadyVerified => {
                Self::failed("Contract source code already verified".into())
            }
            _ => Self::failed(api_error.message()),
        }
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

    fn validate_compilers(
        versions: &CompilerVersions,
        bytecode_kind: BytecodeMarker,
    ) -> Result<(), ApiError> {
        match bytecode_kind {
            BytecodeMarker::EraVm if versions.zk_compiler_version().is_none() => {
                Err(ApiError::MissingZkCompilerVersion)
            }
            BytecodeMarker::Evm if versions.zk_compiler_version().is_some() => {
                Err(ApiError::BogusZkCompilerVersion)
            }
            _ => Ok(()),
        }
    }

    /// Handler to process ZKsync verification requests and Etherscan-like requests for multiple actions.
    #[tracing::instrument(skip(self_, request))]
    pub async fn post(
        State(self_): State<Arc<Self>>,
        request: Request<Body>,
    ) -> ApiResult<APIPostResponse> {
        let (parts, body) = request.into_parts();
        let headers: HeaderMap = parts.headers;

        let content_type = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let body_bytes = to_bytes(body, usize::MAX)
            .await
            .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;

        match content_type {
            // ZKsync verification request in JSON format
            "application/json" => {
                let req: VerificationIncomingRequest = serde_json::from_slice(&body_bytes)
                    .map_err(|e| ApiError::DeserializationError(anyhow::anyhow!(e)))?;
                let verification_id = Self::verification(State(self_), req).await?;
                return Ok(Json(APIPostResponse::VerificationId(verification_id)));
            }
            // Etherscan-like verification request in urlencoded form format
            "application/x-www-form-urlencoded" => {
                let req: EtherscanPostRequest =
                    serde_urlencoded::from_bytes::<EtherscanPostRequest>(&body_bytes)
                        .map_err(|e| ApiError::DeserializationError(anyhow::anyhow!(e)))?;

                let etherscan_response = Self::etherscan_post_action(State(self_), req).await?;
                return Ok(Json(APIPostResponse::EtherscanResponse(etherscan_response)));
            }
            _ => return Err(ApiError::UnsupportedContentType),
        }
    }

    async fn etherscan_get_abi(self: Arc<Self>, address: Address) -> EtherscanResponse {
        match Self::verification_info(State(self), Path(address)).await {
            // Return the abi only for the target address, omit other matches.
            Ok(verification_info) if verification_info.request.req.contract_address == address => {
                EtherscanResponse::successful(verification_info.0.artifacts.abi.to_string())
            }
            _ => EtherscanResponse::failed("Contract source code not verified".into()),
        }
    }

    async fn etherscan_get_source_code(self: Arc<Self>, address: Address) -> EtherscanResponse {
        let verification_info = Self::verification_info(State(self), Path(address))
            .await
            .ok()
            .map(|info| info.0);
        let response = match verification_info {
            // Return the source code only for the target address, omit other matches.
            Some(verification_info)
                if verification_info.request.req.contract_address == address =>
            {
                Some(verification_info)
            }
            _ => None,
        };
        EtherscanResponse {
            status: "1".to_string(),
            message: "OK".to_string(),
            result: EtherscanResult::SourceCode(response.into()),
        }
    }

    async fn etherscan_check_verification_status(
        self: Arc<Self>,
        verification_id: usize,
    ) -> EtherscanResponse {
        match Self::verification_request_status(State(self), Path(verification_id)).await {
            Ok(verification_status) => verification_status.0.into(),
            _ => EtherscanResponse::failed("Unable to locate guid".into()),
        }
    }

    /// General handler to process Etherscan-like GET requests.
    #[tracing::instrument(skip(self_, query))]
    pub async fn etherscan_get_action(
        State(self_): State<Arc<Self>>,
        Query(query): Query<EtherscanGetParams>,
    ) -> ApiResult<EtherscanResponse> {
        let method_latency = METRICS.call[&"contract_verification_etherscan_get"].start();
        let payload = match query.get_payload() {
            Ok(payload) => payload,
            Err(err) => {
                return Ok(Json(EtherscanResponse::failed(err.to_string())));
            }
        };

        let response = match payload {
            EtherscanGetPayload::GetAbi(address) => Self::etherscan_get_abi(self_, address).await,
            EtherscanGetPayload::GetSourceCode(address) => {
                Self::etherscan_get_source_code(self_, address).await
            }
            EtherscanGetPayload::CheckVerifyStatus(verification_id) => {
                Self::etherscan_check_verification_status(self_, verification_id).await
            }
        };

        method_latency.observe();
        Ok(Json(response))
    }

    /// General handler to process Etherscan-like POST requests. Based on the EtherscanPostPayload
    /// variant, it either checks the verification status or verifies the source code.
    #[tracing::instrument(skip(self_, request))]
    async fn etherscan_post_action(
        State(self_): State<Arc<Self>>,
        request: EtherscanPostRequest,
    ) -> Result<EtherscanResponse, ApiError> {
        match request.payload {
            EtherscanPostPayload::CheckVerifyStatus { guid } => {
                let verification_id = guid.parse::<usize>().map_err(|_| {
                    ApiError::DeserializationError(anyhow::anyhow!(
                        "Invalid verification id format. A number is expected."
                    ))
                })?;
                let Json(status) =
                    Self::verification_request_status(State(self_), Path(verification_id)).await?;
                return Ok(status.into());
            }
            EtherscanPostPayload::VerifySourceCode(verification_req) => {
                let verification_id = Self::verification(
                    State(self_),
                    verification_req
                        .to_verification_request()
                        .map_err(ApiError::DeserializationError)?,
                )
                .await;
                match verification_id {
                    Ok(id) => {
                        return Ok(EtherscanResponse::successful(id.to_string()));
                    }
                    // For Etherscan-like requests we need to return the error message as 200 OK
                    Err(err) => {
                        return Ok(err.into());
                    }
                }
            }
        }
    }

    /// Add a contract verification job to the queue if the requested contract wasn't previously verified.
    #[tracing::instrument(skip(self_, request))]
    async fn verification(
        State(self_): State<Arc<Self>>,
        request: VerificationIncomingRequest,
    ) -> Result<usize, ApiError> {
        let method_latency = METRICS.call[&"contract_verification"].start();
        Self::validate_contract_verification_query(&request)?;

        let is_compilation_supported = self_
            .supported_compilers
            .get(|supported| supported.contain(&request.compiler_versions))
            .await?;
        if !is_compilation_supported {
            return Err(ApiError::UnsupportedCompilerVersions);
        }

        let mut storage = self_
            .master_connection_pool
            .connection_tagged("api")
            .await?;

        // Verification is only allowed if the contract is either wasn't verified yet
        // or the verification is partial.
        let verification_info = storage
            .contract_verification_dal()
            .get_contract_verification_info(request.contract_address)
            .await?;
        if let Some(verification_info) = verification_info {
            let fully_verified = verification_info.verification_problems.is_empty();
            // System contracts can be force deployed during an upgrade, so it should be possible
            // to re-verify them.
            let is_system = match &verification_info.request.req.source_code_data {
                SourceCodeData::SolSingleFile(_) | SourceCodeData::YulSingleFile(_) => {
                    verification_info.request.req.is_system
                }
                SourceCodeData::StandardJsonInput(input) => input
                    .get("settings")
                    .and_then(|s| s.get("isSystem").or_else(|| s.get("enableEraVMExtensions")))
                    .and_then(|s| s.as_bool())
                    .unwrap_or(false),
                _ => false,
            };

            if fully_verified && !is_system {
                return Err(ApiError::AlreadyVerified);
            }
        }
        // Check if there is already a verification request for this contract.
        if let Some(id) = storage
            .contract_verification_dal()
            .get_active_verification_request(request.contract_address)
            .await?
        {
            return Err(ApiError::ActiveRequestExists(id));
        }

        let deployment_info = storage
            .storage_logs_dal()
            .filter_deployed_contracts(iter::once(request.contract_address), None)
            .await?;
        let &(_, bytecode_hash) = deployment_info
            .get(&request.contract_address)
            .ok_or(ApiError::NoDeployedContract)?;
        let bytecode_marker = BytecodeMarker::new(bytecode_hash).with_context(|| {
            format!(
                "unknown bytecode marker for bytecode hash {bytecode_hash:?} at address {:?}",
                request.contract_address
            )
        })?;
        Self::validate_compilers(&request.compiler_versions, bytecode_marker)?;

        let request_id = storage
            .contract_verification_dal()
            .add_contract_verification_request(&request)
            .await?;
        method_latency.observe();
        Ok(request_id)
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
    pub async fn zksolc_versions(State(self_): State<Arc<Self>>) -> ApiResult<HashSet<String>> {
        let method_latency = METRICS.call[&"contract_verification_zksolc_versions"].start();
        let versions = self_
            .supported_compilers
            .get(|supported| supported.zksolc.clone())
            .await?;
        method_latency.observe();
        Ok(Json(versions))
    }

    #[tracing::instrument(skip(self_))]
    pub async fn solc_versions(State(self_): State<Arc<Self>>) -> ApiResult<HashSet<String>> {
        let method_latency = METRICS.call[&"contract_verification_solc_versions"].start();
        let versions = self_
            .supported_compilers
            .get(|supported| supported.solc.clone())
            .await?;
        method_latency.observe();
        Ok(Json(versions))
    }

    #[tracing::instrument(skip(self_))]
    pub async fn zkvyper_versions(State(self_): State<Arc<Self>>) -> ApiResult<HashSet<String>> {
        let method_latency = METRICS.call[&"contract_verification_zkvyper_versions"].start();
        let versions = self_
            .supported_compilers
            .get(|supported| supported.zkvyper.clone())
            .await?;
        method_latency.observe();
        Ok(Json(versions))
    }

    #[tracing::instrument(skip(self_))]
    pub async fn vyper_versions(State(self_): State<Arc<Self>>) -> ApiResult<HashSet<String>> {
        let method_latency = METRICS.call[&"contract_verification_vyper_versions"].start();
        let versions = self_
            .supported_compilers
            .get(|supported| supported.vyper.clone())
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
        let mut conn = self_
            .replica_connection_pool
            .connection_tagged("api")
            .await?;
        let mut dal = conn.contract_verification_dal();

        let info = if let Some(info) = dal.get_contract_verification_info(*address).await? {
            info
        } else if let Some(partial_match) =
            get_partial_match_verification_info(&mut dal, *address).await?
        {
            partial_match
        } else {
            return Err(ApiError::VerificationInfoNotFound);
        };
        method_latency.observe();
        Ok(Json(info))
    }
}

/// Tries to do a lookup for partial match verification info.
/// Should be called only if a perfect match is not found.
async fn get_partial_match_verification_info(
    dal: &mut ContractVerificationDal<'_, '_>,
    address: Address,
) -> anyhow::Result<Option<VerificationInfo>> {
    let Some(deployed_contract) = dal.get_contract_info_for_verification(address).await? else {
        return Ok(None);
    };

    let bytecode_hash =
        BytecodeHash::try_from(deployed_contract.bytecode_hash).context("Invalid bytecode hash")?;
    let deployed_bytecode = trim_bytecode(bytecode_hash, &deployed_contract.bytecode)
        .context("Invalid deployed bytecode")?;

    let identifier = ContractIdentifier::from_bytecode(bytecode_hash.marker(), deployed_bytecode);
    let Some((mut info, fetched_keccak256, fetched_keccak256_without_metadata)) = dal
        .get_partial_match_verification_info(
            identifier.bytecode_keccak256,
            identifier.bytecode_without_metadata_keccak256,
        )
        .await?
    else {
        return Ok(None);
    };

    if identifier.bytecode_keccak256 != fetched_keccak256 {
        // Sanity check
        let has_metadata = identifier.detected_metadata.is_some();
        let hashes_without_metadata_match =
            identifier.bytecode_without_metadata_keccak256 == fetched_keccak256_without_metadata;

        if !has_metadata || !hashes_without_metadata_match {
            tracing::error!(
                contract_address = ?address,
                identifier = ?identifier,
                fetched_keccak256 = ?fetched_keccak256,
                fetched_keccak256_without_metadata = ?fetched_keccak256_without_metadata,
                info = ?info,
                "Bogus verification info fetched for contract",
            );
            anyhow::bail!("Internal error: bogus verification info detected");
        }

        // Mark the contract as partial match (regardless of other issues).
        info.verification_problems = vec![VerificationProblem::IncorrectMetadata];
    }

    Ok(Some(info))
}
