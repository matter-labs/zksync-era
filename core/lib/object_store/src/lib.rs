//! This crate provides the [object storage abstraction](ObjectStore) that allows to get,
//! put and remove binary blobs. The following implementations are available:
//!
//! - File-based storage saving blobs as separate files in the local filesystem
//! - GCS-based storage
//!
//! These implementations are not exposed externally. Instead, a store trait object
//! can be constructed using an [`ObjectStoreFactory`] based on the configuration.
//! The configuration can be provided explicitly (see [`ObjectStoreFactory::new()`])
//! or obtained from the environment (see [`ObjectStoreFactory::from_env()`]).
//!
//! Besides the lower-level storage abstraction, the crate provides high-level
//! typesafe `<dyn ObjectStore>::get()` and `<dyn ObjectStore>::put()` methods
//! to store [(de)serializable objects](StoredObject). Prefer using these methods
//! whenever possible.

// Linter settings.
#![warn(missing_debug_implementations, bare_trait_objects)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::must_use_candidate,
    clippy::module_name_repetitions,
    clippy::doc_markdown
)]

mod file;
mod gcs;
mod metrics;
mod mock;
mod objects;
mod raw;

// Re-export `bincode` crate so that client binaries can conveniently use it.
pub use bincode;

#[doc(hidden)] // used by the `serialize_using_bincode!` macro
pub mod _reexports {
    pub use crate::raw::BoxedError;
}

pub use self::{
    objects::{AggregationsKey, CircuitKey, ClosedFormInputKey, FriCircuitKey, StoredObject},
    raw::{Bucket, ObjectStore, ObjectStoreError, ObjectStoreFactory},
};
