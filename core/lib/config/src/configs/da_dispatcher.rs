use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum ValidiumDAMode {
    #[default]
    NoDA,
    Celestia,
    EigenDA,
    Avail,
    GCS,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DADispatcherConfig {
    pub mode: ValidiumDAMode,
    pub da_api_url: String,
}
