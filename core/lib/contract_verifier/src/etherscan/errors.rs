use zksync_dal::DalError;

#[derive(Debug, thiserror::Error)]
pub(super) enum EtherscanError {
    #[error("Error while sending an HTTP request")]
    Reqwest(#[from] reqwest::Error),
    #[error("Failed to deserialize content: {error}\n{content}")]
    Serde {
        error: serde_json::Error,
        content: String,
    },
    #[error("Contract bytecode is not available")]
    ContractBytecodeNotAvailable,
    #[error("Contract source code not verified")]
    ContractNotVerified,
    #[error("Contract source code already verified")]
    ContractAlreadyVerified,
    #[error("Contract verification is pending in queue")]
    VerificationPending,
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("Exceeded daily verifications limit")]
    DailyVerificationRequestsLimitExceeded,
    #[error("Received error response: message={message}, result={result}")]
    ErrorResponse { message: String, result: String },
    #[error("Etherscan API key is invalid even though it has no expiration date. Either Etherscan API is experiencing issues or the key was revoked")]
    InvalidApiKey,
    #[error("The request has been blocked by Cloudflare")]
    BlockedByCloudflare,
    #[error("The request prompted a Cloudflare captcha security challenge")]
    CloudflareSecurityChallenge,
    #[error("Received `Page not found` response. API server is likely down")]
    PageNotFound,
    #[error("Unexpected API response: message={message}, status={status}")]
    UnexpectedResponse { message: String, status: String },
}

impl EtherscanError {
    pub fn unexpected_response(message: String, status: String) -> Self {
        Self::UnexpectedResponse { message, status }
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum ProcessingError {
    #[error("Get verification status timed out. Failing the verification process")]
    VerificationStatusPollingTimeout,
    #[error("Database related error")]
    DalError(#[from] DalError),
    #[error("Unsupported source code data format")]
    UnsupportedSourceCodeFormat,
    #[error("Unsupported compiler version")]
    UnsupportedCompilerVersion,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum VerifierError {
    #[error("Error during Etherscan API request: {0}")]
    EtherscanError(EtherscanError),
    #[error("The execution has been canceled")]
    ProcessingError(ProcessingError),
    #[error("The execution has been canceled")]
    Canceled,
}

impl From<EtherscanError> for VerifierError {
    fn from(err: EtherscanError) -> Self {
        Self::EtherscanError(err)
    }
}

impl From<DalError> for VerifierError {
    fn from(err: DalError) -> Self {
        Self::ProcessingError(ProcessingError::DalError(err))
    }
}

impl From<ProcessingError> for VerifierError {
    fn from(err: ProcessingError) -> Self {
        Self::ProcessingError(err)
    }
}
