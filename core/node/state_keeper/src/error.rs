#[derive(Debug, thiserror::Error)]
pub enum ProcessBlockError {
    #[error("{0}")]
    Internal(#[from] anyhow::Error),
    #[error("block verification failed")]
    BlockVerificationFailed,
}

impl ProcessBlockError {
    pub fn unwrap_internal(self) -> anyhow::Error {
        match self {
            Self::Internal(err) => err,
            _ => panic!("called unwrap_internal on a non-internal error"),
        }
    }
}
