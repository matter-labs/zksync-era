//! Metadata information about the external node.

use vise::EncodeLabelSet;

pub(crate) use self::values::RUSTC_METADATA;

mod values {
    use super::RustcMetadata;
    include!(concat!(env!("OUT_DIR"), "/metadata_values.rs"));
}

#[derive(Debug, EncodeLabelSet)]
pub(crate) struct RustcMetadata {
    pub version: &'static str,
    pub commit_hash: Option<&'static str>,
    pub commit_date: Option<&'static str>,
    pub channel: &'static str,
    pub host: &'static str,
    pub llvm: Option<&'static str>,
}

pub(crate) const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
