use std::rc::Rc;

pub use full_builder::FullPubdataBuilder;
pub use hashed_builder::HashedPubdataBuilder;
use zksync_types::commitment::{DAClientType, L1BatchCommitmentMode, PubdataParams};

use crate::interface::pubdata::PubdataBuilder;

mod full_builder;
mod hashed_builder;
#[cfg(test)]
mod tests;
mod utils;

pub fn pubdata_params_to_builder(params: PubdataParams) -> Rc<dyn PubdataBuilder> {
    match params.da_client_type {
        Some(DAClientType::NoDA) => {
            Rc::new(HashedPubdataBuilder::new(params.l2_da_validator_address))
        }
        _ => Rc::new(FullPubdataBuilder::new(params.l2_da_validator_address)),
    }
}
