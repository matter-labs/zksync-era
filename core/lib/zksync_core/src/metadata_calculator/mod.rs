//! This module applies updates to the ZkSyncTree, calculates metadata for sealed blocks, and
//! stores them in the DB.

mod helpers;
mod metrics;

pub(crate) use self::helpers::L1BatchWithLogs;
