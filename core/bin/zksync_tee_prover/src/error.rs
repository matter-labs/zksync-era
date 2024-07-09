use std::{error::Error as StdError, io};

use reqwest::StatusCode;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TeeProverError {
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error(transparent)]
    Verification(anyhow::Error),
}

impl TeeProverError {
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Request(err) => is_transient_http_error(err),
            _ => false,
        }
    }
}

fn is_transient_http_error(err: &reqwest::Error) -> bool {
    err.is_timeout()
            || err.is_connect()
            // Not all request errors are logically transient, but a significant part of them are (e.g.,
            // `hyper` protocol-level errors), and it's safer to consider an error transient.
            || err.is_request()
            || has_transient_io_source(err)
            || err.status() == Some(StatusCode::BAD_GATEWAY)
            || err.status() == Some(StatusCode::SERVICE_UNAVAILABLE)
}

fn has_transient_io_source(err: &(dyn StdError + 'static)) -> bool {
    // We treat any I/O errors as transient. This isn't always true, but frequently occurring I/O errors
    // (e.g., "connection reset by peer") *are* transient, and treating an error as transient is a safer option,
    // even if it can lead to unnecessary retries.
    get_source::<io::Error>(err).is_some()
}

fn get_source<'a, T: StdError + 'static>(mut err: &'a (dyn StdError + 'static)) -> Option<&'a T> {
    loop {
        if let Some(err) = err.downcast_ref::<T>() {
            return Some(err);
        }
        err = err.source()?;
    }
}
