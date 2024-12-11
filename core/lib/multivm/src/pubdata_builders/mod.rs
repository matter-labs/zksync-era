use std::rc::Rc;

pub use rollup::RollupPubdataBuilder;
pub use validium::ValidiumPubdataBuilder;
use zksync_types::commitment::{L1BatchCommitmentMode, PubdataParams};

use crate::interface::pubdata::PubdataBuilder;

mod rollup;
#[cfg(test)]
mod tests;
mod utils;
mod validium;

pub fn pubdata_params_to_builder(params: PubdataParams) -> Rc<dyn PubdataBuilder> {
    match params.pubdata_type {
        L1BatchCommitmentMode::Rollup => {
            Rc::new(RollupPubdataBuilder::new(params.l2_da_validator_address))
        }
        L1BatchCommitmentMode::Validium => {
            Rc::new(ValidiumPubdataBuilder::new(params.l2_da_validator_address))
        }
    }
}
