//! Backend "glue" which ties the actual Web3 API implementation to the `jsonrpsee` JSON RPC backend.
//! Consists mostly of boilerplate code implementing the `jsonrpsee` server traits for the corresponding
//! namespace structures defined in `zksync_core`.

pub mod namespaces;
