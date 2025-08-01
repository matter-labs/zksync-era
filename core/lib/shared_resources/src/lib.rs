//! Shared dependency injection code for ZKsync node.

use zksync_node_framework::Resource;
use zksync_types::{commitment::L1BatchCommitmentMode, pubdata_da::PubdataSendingMode};

pub mod api;
pub mod contracts;
pub mod tree;

#[derive(Debug, Clone, Copy)]
pub struct PubdataSendingModeResource(pub PubdataSendingMode);

impl Resource for PubdataSendingModeResource {
    fn name() -> String {
        "common/pubdata_sending_mode".into()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DummyVerifierResource(pub bool);

impl Resource for DummyVerifierResource {
    fn name() -> String {
        "common/dummy_verifier".into()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct L1BatchCommitmentModeResource(pub L1BatchCommitmentMode);
impl Resource for L1BatchCommitmentModeResource {
    fn name() -> String {
        "common/l1_batch_commitment_mode".into()
    }
}
