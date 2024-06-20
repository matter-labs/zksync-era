use vise::{EncodeLabelSet, Info, Metrics};

mod values {
    use super::RustcMetadata;
    include!(concat!(env!("OUT_DIR"), "/metadata_values.rs"));
}

use values::RUSTC_METADATA;

/// Metadata of Rust compiler used to compile the crate.
#[derive(Debug, EncodeLabelSet)]
pub struct RustcMetadata {
    pub version: &'static str,
    pub commit_hash: Option<&'static str>,
    pub commit_date: Option<&'static str>,
    pub channel: &'static str,
    pub host: &'static str,
    pub llvm: Option<&'static str>,
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
