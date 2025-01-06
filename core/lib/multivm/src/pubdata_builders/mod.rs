use std::rc::Rc;

pub use full_builder::FullPubdataBuilder;
pub use hashed_builder::HashedPubdataBuilder;
use zksync_types::commitment::{L1BatchCommitmentMode, PubdataParams};

use crate::interface::pubdata::PubdataBuilder;

mod full_builder;
mod hashed_builder;
#[cfg(test)]
mod tests;
mod utils;

pub fn pubdata_params_to_builder(params: PubdataParams) -> Rc<dyn PubdataBuilder> {
    match params.pubdata_type {
        // hashed builder can only be used in NoDA validium, but with
        // `pubdata_type: L1BatchCommitmentMode` we have no way of knowing which type of Validium
        // is used, so let's use `FullPubdataBuilder` for all cases here
        L1BatchCommitmentMode::Rollup | L1BatchCommitmentMode::Validium => {
            Rc::new(FullPubdataBuilder::new(params.l2_da_validator_address))
        }
    }
}
