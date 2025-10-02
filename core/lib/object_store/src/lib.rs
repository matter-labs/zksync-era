//! This crate provides the [object storage abstraction](ObjectStore) that allows to get,
//! put and remove binary blobs. The following implementations are available:
//!
//! - [File-backed store](FileBackedObjectStore) saving blobs as separate files in the local filesystem
//! - [GCS-based store](GoogleCloudStore)
//! - [Mock in-memory store](MockObjectStore)
//!
//! Normally, these implementations are not used directly. Instead, a store trait object (`Arc<dyn ObjectStore>`)
//! can be constructed using an [`ObjectStoreFactory`] based on the configuration.
//! This trait object is what should be used for dependency injection.
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

mod factory;
mod file;
mod gcs;
mod metrics;
mod mirror;
mod mock;
#[cfg(feature = "node_framework")]
pub mod node;
mod objects;
mod raw;
mod retries;
mod s3;

// Re-export `bincode` crate so that client binaries can conveniently use it.
pub use bincode;

#[doc(hidden)] // used by the `serialize_using_bincode!` macro
pub mod _reexports {
    pub use crate::raw::BoxedError;
}

pub use self::{
    factory::ObjectStoreFactory,
    file::FileBackedObjectStore,
    gcs::{GoogleCloudStore, GoogleCloudStoreAuthMode},
    mock::MockObjectStore,
    objects::StoredObject,
    raw::{Bucket, ObjectStore, ObjectStoreError},
};
