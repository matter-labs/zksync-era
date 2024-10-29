use std::sync::Arc;

use memstore::MemStore;
use remote::RemoteClient;

pub mod blob_info;
pub mod common;
pub mod disperser;
pub mod memstore;
pub mod remote;

#[derive(Clone, Debug)]
pub(crate) enum Disperser {
    Remote(RemoteClient),
    Memory(Arc<MemStore>),
}
