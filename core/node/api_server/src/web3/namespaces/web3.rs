#[derive(Debug, Clone)]
pub struct Web3Namespace;

impl Web3Namespace {
    pub fn client_version_impl(&self) -> String {
        "zkSync/v2.0".to_string()
    }
}

// `sha3` method is intentionally not implemented for the main server implementation:
// it can easily be implemented on the user side.
