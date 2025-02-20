use zksync_dal::DalError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ApiError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("Failed to deserialize content: {error}\n{content}")]
    Serde {
        error: serde_json::Error,
        content: String,
    },
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
    #[error("Etherscan API key is invalid even though it has no expiration date. Either Etherscan API is experiencing issues or the key was revoked.")]
    InvalidApiKey,
    #[error("The request has been blocked by Cloudflare.")]
    BlockedByCloudflare,
    #[error("The request prompted a Cloudflare captcha security challenge.")]
    CloudFlareSecurityChallenge,
    #[error("Received `Page not found` response. API server is likely down")]
    PageNotFound,
    #[error("Unexpected API response: message={message}, result={result}")]
    UnexpectedResponse { message: String, result: String },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ProcessingError {
    #[error("Get verification status timed out. Failing the verification process.")]
    VerificationStatusPollingTimeout,
    #[error("Database related error")]
    DalError(#[from] DalError),
}

pub(crate) enum VerifierError {
    ApiError(ApiError),
    ProcessingError(ProcessingError),
}

impl From<ApiError> for VerifierError {
    fn from(err: ApiError) -> Self {
        Self::ApiError(err)
    }
}

impl From<ProcessingError> for VerifierError {
    fn from(err: ProcessingError) -> Self {
        Self::ProcessingError(err)
    }
}

pub(crate) fn is_blocked_by_cloudflare_response(txt: &str) -> bool {
    txt.to_lowercase().contains("sorry, you have been blocked")
}

pub(crate) fn is_cloudflare_security_challenge(txt: &str) -> bool {
    txt.contains("https://www.cloudflare.com?utm_source=challenge")
        || txt
            .to_lowercase()
            .contains("checking if the site connection is secure")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_cloudflare_security_challenge() {
        let res = "Security challenge link https://www.cloudflare.com?utm_source=challenge";
        assert!(is_cloudflare_security_challenge(res));

        let res = "Checking if the site connection is secure...";
        assert!(is_cloudflare_security_challenge(res));
    }

    #[test]
    fn test_is_blocked_by_cloudflare_response() {
        let res = "Sorry, you have been blocked...";
        assert!(is_blocked_by_cloudflare_response(res));
    }
}
