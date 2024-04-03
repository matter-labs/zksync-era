use std::sync::Arc;

use zksync_core::eth_sender::l1_batch_commit_data_generator::L1BatchCommitDataGenerator;

use crate::resource::Resource;

#[derive(Debug, Clone)]
pub struct L1BatchCommitDataGeneratorResource(pub Arc<dyn L1BatchCommitDataGenerator>);

impl Resource for L1BatchCommitDataGeneratorResource {
    fn name() -> String {
        "common/l1_batch_commit_data_generator".into()
    }
}
