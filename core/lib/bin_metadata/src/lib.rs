use serde::Serialize;
use vise::{EncodeLabelSet, Info, Metrics};

use self::values::BIN_METADATA;

pub mod values {
    use super::BinMetadata;
    include!(concat!(env!("OUT_DIR"), "/metadata_values.rs"));
}

/// Metadata of the compiled binary.
#[derive(Debug, EncodeLabelSet, Serialize)]
pub struct BinMetadata {
    pub rustc_version: &'static str,
    pub rustc_commit_hash: Option<&'static str>,
    pub rustc_commit_date: Option<&'static str>,
    pub rustc_channel: &'static str,
    pub host: &'static str,
    pub llvm: Option<&'static str>,
    pub git_branch: Option<&'static str>,
    pub git_revision: Option<&'static str>,
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
