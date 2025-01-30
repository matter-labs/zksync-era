use std::{collections::HashSet, iter, sync::Arc};

use anyhow::Context as _;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use zksync_dal::{contract_verification_dal::ContractVerificationDal, CoreDal, DalError};
use zksync_types::{
    bytecode::{trim_bytecode, BytecodeHash, BytecodeMarker},
    contract_verification::{
        api::{
            CompilerVersions, VerificationIncomingRequest, VerificationInfo, VerificationProblem,
            VerificationRequestStatus,
        },
        contract_identifier::ContractIdentifier,
    },
    Address,
};

use super::{api_decl::RestApi, metrics::METRICS};

#[derive(Debug)]
pub(crate) enum ApiError {
    IncorrectCompilerVersions,
    UnsupportedCompilerVersions,
    MissingZkCompilerVersion,
    BogusZkCompilerVersion,
    NoDeployedContract,
    RequestNotFound,
    VerificationInfoNotFound,
    Internal(anyhow::Error),
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
    pub fn message(&self) -> &'static str {
        match self {
            Self::IncorrectCompilerVersions => "incorrect compiler versions",
            Self::UnsupportedCompilerVersions => "unsupported compiler versions",
            Self::MissingZkCompilerVersion => "missing zk compiler version for EraVM bytecode",
            Self::BogusZkCompilerVersion => "zk compiler version specified for EVM bytecode",
            Self::NoDeployedContract => "There is no deployed contract on this address",
            Self::RequestNotFound => "request not found",
            Self::VerificationInfoNotFound => "verification info not found for address",
            Self::Internal(_) => "internal server error",
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
            | Self::NoDeployedContract => StatusCode::BAD_REQUEST,

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

    /// Add a contract verification job to the queue if the requested contract wasn't previously verified.
    // FIXME: this doesn't seem to check that the contract isn't verified; should it?
    #[tracing::instrument(skip(self_, request))]
    pub async fn verification(
        State(self_): State<Arc<Self>>,
        Json(request): Json<VerificationIncomingRequest>,
    ) -> ApiResult<usize> {
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
