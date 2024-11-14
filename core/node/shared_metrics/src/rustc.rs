use async_trait::async_trait;
use serde::Serialize;
use vise::{EncodeLabelSet, Info, Metrics};

mod values {
    use super::RustcMetadata;
    include!(concat!(env!("OUT_DIR"), "/metadata_values.rs"));
}

pub use values::RUSTC_METADATA;
use zksync_health_check::{CheckHealth, Health, HealthStatus};

/// Metadata of Rust compiler used to compile the crate.
#[derive(Debug, EncodeLabelSet, Serialize)]
pub struct RustcMetadata {
    pub version: &'static str,
    pub commit_hash: Option<&'static str>,
    pub commit_date: Option<&'static str>,
    pub channel: &'static str,
    pub host: &'static str,
    pub llvm: Option<&'static str>,
    pub git_branch: &'static str,
    pub git_revision: &'static str,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "rust")]
pub struct RustMetrics {
    /// General information about the Rust compiler.
    info: Info<RustcMetadata>,
}

impl RustMetrics {
    pub fn initialize(&self) {
        tracing::info!("Metadata for rustc that this binary was compiled with: {RUSTC_METADATA:?}");
        self.info.set(RUSTC_METADATA).ok();
    }
}

#[vise::register]
pub static RUST_METRICS: vise::Global<RustMetrics> = vise::Global::new();

impl From<&RustcMetadata> for Health {
    fn from(details: &RustcMetadata) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

#[async_trait]
impl CheckHealth for RustcMetadata {
    fn name(&self) -> &'static str {
        "rustc_metadata"
    }

    async fn check_health(&self) -> Health {
        self.into()
    }
}
