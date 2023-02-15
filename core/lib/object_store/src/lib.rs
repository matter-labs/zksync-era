extern crate core;

pub mod file_backed_object_store;
pub mod gcs_object_store;
pub mod object_store;
pub use cloud_storage;

pub mod gcs_utils;
#[cfg(test)]
mod tests;
