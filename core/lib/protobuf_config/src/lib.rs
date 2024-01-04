//! Defined protobuf mapping for the config files.
//! It allows to encode the configs using:
//! * protobuf binary format
//! * protobuf text format
//! * protobuf json format

mod alerts;
mod api;
pub mod proto;
mod repr;
#[cfg(test)]
mod tests;
mod utils;
