use anyhow::anyhow;
use zksync_da_client::types::DAError;

pub fn to_non_retriable_da_error<E>(error: E) -> DAError
where
    E: std::fmt::Display,
{
    DAError {
        error: anyhow!(error.to_string()),
        is_retriable: false,
    }
}

pub fn to_retriable_da_error<E>(error: E) -> DAError
where
    E: std::fmt::Display,
{
    DAError {
        error: anyhow!(error.to_string()),
        is_retriable: true,
    }
}
