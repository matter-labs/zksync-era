use async_trait::async_trait;
use serde::Serialize;
use vise::{EncodeLabelSet, Info, Metrics};

mod values {
    use super::BinMetadata;
    include!(concat!(env!("OUT_DIR"), "/metadata_values.rs"));
}

pub use values::BIN_METADATA;
use zksync_health_check::{CheckHealth, Health, HealthStatus};

/// Metadata of the compiled binary.
#[derive(Debug, EncodeLabelSet, Serialize)]
pub struct BinMetadata {
    pub rustc_version: &'static str,
    pub rustc_commit_hash: Option<&'static str>,
    pub rustc_commit_date: Option<&'static str>,
    pub rustc_channel: &'static str,
    pub host: &'static str,
    pub llvm: Option<&'static str>,
    pub git_branch: &'static str,
    pub git_revision: &'static str,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "rust")]
pub struct BinMetrics {
    /// General information about the compiled binary.
    info: Info<BinMetadata>,
}

impl BinMetrics {
    pub fn initialize(&self) {
        tracing::info!("Metadata for this binary: {BIN_METADATA:?}");
        self.info.set(BIN_METADATA).ok();
    }
}

#[vise::register]
pub static BIN_METRICS: vise::Global<BinMetrics> = vise::Global::new();

impl From<&BinMetadata> for Health {
    fn from(details: &BinMetadata) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

#[async_trait]
impl CheckHealth for BinMetadata {
    fn name(&self) -> &'static str {
        "metadata"
    }

    async fn check_health(&self) -> Health {
        self.into()
    }
}
