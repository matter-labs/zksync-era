//! Miscellaneous VM utils.

pub use self::{
    dump::VmDump,
    shadow::{DivergenceErrors, DivergenceHandler, ShadowVm},
};

mod dump;
mod shadow;
