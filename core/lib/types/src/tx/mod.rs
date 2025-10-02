//! `transactions` is module that holds the essential information for every transaction.
//!
//! Since in ZKsync Era every operation can be executed either from the contract or rollup,
//! it makes more sense to define the contents of each transaction chain-agnostic, and extent this data
//! with metadata (such as fees and/or signatures) for L1 and L2 separately.

use zksync_basic_types::H256;

pub use self::execute::Execute;

pub mod execute;
pub use zksync_crypto_primitives as primitives;

#[derive(Debug, Clone, Copy)]
pub struct IncludedTxLocation {
    pub tx_hash: H256,
    pub tx_index_in_l2_block: u32,
}
