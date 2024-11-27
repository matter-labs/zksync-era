use serde::Serialize;
use vise::{EncodeLabelSet, Info, Metrics};

use self::values::{GIT_METADATA, RUST_METADATA};

mod values {
    use super::{GitMetadata, RustMetadata};

    include!(concat!(env!("OUT_DIR"), "/metadata_values.rs"));
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
    pub fn initialize(&self) -> &RustMetadata {
        tracing::info!("Rust metadata for this binary: {RUST_METADATA:?}");
        self.info.set(RUST_METADATA).ok();
        // `unwrap` is safe due to setting the value above
        self.info.get().unwrap()
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "git")]
pub struct GitMetrics {
    /// General information about the compiled binary.
    info: Info<GitMetadata>,
}

impl GitMetrics {
    pub fn initialize(&self) -> &GitMetadata {
        tracing::info!("Git metadata for this binary: {GIT_METADATA:?}");
        self.info.set(GIT_METADATA).ok();
        // `unwrap` is safe due to setting the value above
        self.info.get().unwrap()
    }
}

#[vise::register]
pub static RUST_METRICS: vise::Global<RustMetrics> = vise::Global::new();

#[vise::register]
pub static GIT_METRICS: vise::Global<GitMetrics> = vise::Global::new();
