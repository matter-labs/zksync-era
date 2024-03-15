pub mod batch_status_updater;
mod client;
pub mod external_io;
pub mod fetcher;
pub mod genesis;
mod metrics;
pub(crate) mod sync_action;
mod sync_state;
#[cfg(test)]
mod tests;

use zksync_types::U256;

pub use self::{
    client::MainNodeClient, external_io::ExternalIO, sync_action::ActionQueue,
    sync_state::SyncState,
};

// These config values are used on the main node, and depending on these values certain transactions can
// be *rejected* (that is, not included into the block). However, external node only mirrors what the main
// node has already executed, so we can safely set these values to the maximum possible values - if the main
// node has already executed the transaction, then the external node must execute it too.
/// Gas limit for L2 transactions for the external node.
pub const MAX_ALLOWED_L2_TX_GAS_LIMIT: U256 = U256([u32::MAX as u64, 0, 0, 0]);
/// Validation gas limit used by the external node.
const VALIDATION_COMPUTATIONAL_GAS_LIMIT: u32 = u32::MAX;
