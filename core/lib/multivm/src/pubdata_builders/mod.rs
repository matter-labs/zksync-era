use std::rc::Rc;

pub use full_builder::FullPubdataBuilder;
pub use hashed_builder::HashedPubdataBuilder;
use zksync_types::{
    commitment::{PubdataParams, PubdataType},
    ProtocolVersionId,
};

use crate::interface::pubdata::PubdataBuilder;

mod full_builder;
mod hashed_builder;
#[cfg(test)]
mod tests;
mod utils;

pub fn pubdata_params_to_builder(
    params: PubdataParams,
    protocol_version: ProtocolVersionId,
) -> Rc<dyn PubdataBuilder> {
    if protocol_version.is_pre_gateway() {
        return Rc::new(FullPubdataBuilder::new(params.l2_da_validator_address));
    }

    match params.pubdata_type {
        PubdataType::NoDA => Rc::new(HashedPubdataBuilder::new(params.l2_da_validator_address)),
        PubdataType::Rollup
        | PubdataType::Avail
        | PubdataType::Celestia
        | PubdataType::Eigen
        | PubdataType::ObjectStore => {
            Rc::new(FullPubdataBuilder::new(params.l2_da_validator_address))
        }
    }
}
