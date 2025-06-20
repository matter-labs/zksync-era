#![feature(allocator_api, generic_const_exprs)]
#![allow(incomplete_features)]

use serde::{Deserialize, Serialize};

pub mod commitment_utils;
pub mod keystore;
pub mod setup_data_generator;
pub mod utils;
pub mod witness_generator;

#[cfg(feature = "gpu")]
pub mod compressor;

/// Commitments are small 'hashes' generated over the corresponding data.
// We use them as version ids, to make sure that jobs are picking up the right tasks.
#[derive(Debug, Serialize, Deserialize)]
pub struct VkCommitments {
    pub leaf: String,
    pub node: String,
    pub scheduler: String,
    // Hash computed over Snark verification key fields.
    pub snark_wrapper: String,
    pub fflonk_snark_wrapper: String,
}
