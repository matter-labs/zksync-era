pub mod batch_status_updater;
mod client;
pub mod data_availability_fetcher;
pub mod external_io;
pub mod fetcher;
pub mod genesis;
mod metrics;
pub mod node;
pub mod sync_action;
pub mod testonly;
#[cfg(test)]
mod tests;
pub mod tree_data_fetcher;
pub mod validate_chain_ids_task;

pub use self::{
    client::MainNodeClient,
    external_io::ExternalIO,
    sync_action::{ActionQueue, ActionQueueSender},
};

/// Validation gas limit used by the external node.
// This config value is used on the main node, and depending on these values certain transactions can
// be *rejected* (that is, not included into the block). However, external node only mirrors what the main
// node has already executed, so we can safely set this value to the maximum possible values – if the main
// node has already executed the transaction, then the external node must execute it too.
const VALIDATION_COMPUTATIONAL_GAS_LIMIT: u32 = u32::MAX;
