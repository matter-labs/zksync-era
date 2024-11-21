use serde::Serialize;
use vise::{EncodeLabelSet, Info, Metrics};

use self::values::{GIT_METADATA, RUST_METADATA};

pub mod values {
    use super::{GitMetadata, RustMetadata};

    include!(concat!(env!("OUT_DIR"), "/metadata_values.rs"));
}

pub const BIN_METADATA: BinMetadata = BinMetadata {
    rust: RUST_METADATA,
    git: GIT_METADATA,
};

/// Metadata of the compiled binary.
#[derive(Debug, Serialize)]
pub struct BinMetadata {
    pub rust: RustMetadata,
    pub git: GitMetadata,
}

/// Rust metadata of the compiled binary.
#[derive(Debug, EncodeLabelSet, Serialize)]
pub struct RustMetadata {
    pub version: &'static str,
    pub commit_hash: Option<&'static str>,
    pub commit_date: Option<&'static str>,
    pub channel: &'static str,
    pub host: &'static str,
    pub llvm: Option<&'static str>,
}

/// Git metadata of the compiled binary.
#[derive(Debug, EncodeLabelSet, Serialize)]
pub struct GitMetadata {
    pub branch: Option<&'static str>,
    pub revision: Option<&'static str>,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "rust")]
pub struct RustMetrics {
    /// General information about the compiled binary.
    info: Info<RustMetadata>,
}

impl RustMetrics {
    pub fn initialize(&self) {
        tracing::info!("Rust metadata for this binary: {RUST_METADATA:?}");
        self.info.set(RUST_METADATA).ok();
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "git_info")]
pub struct GitMetrics {
    /// General information about the compiled binary.
    info: Info<GitMetadata>,
}

impl GitMetrics {
    pub fn initialize(&self) {
        tracing::info!("Git metadata for this binary: {GIT_METADATA:?}");
        self.info.set(GIT_METADATA).ok();
    }
}
