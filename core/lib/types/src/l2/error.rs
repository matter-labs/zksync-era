use parity_crypto::publickey::Error as ParityCryptoError;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zksync_basic_types::H256;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Error)]
pub enum TxCheckError {
    #[error("transaction type {0} not supported")]
    UnsupportedType(u32),
    #[error("known transaction. transaction with hash {0} is already in the system")]
    TxDuplication(H256),
}

#[derive(Debug, Error)]
pub enum SignError {
    #[error("Failed to sign transaction")]
    SignError(#[from] ParityCryptoError),
}
