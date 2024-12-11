//! Miscellaneous VM utils.

pub use self::{
    dump::VmDump,
    shadow::{
        CheckDivergence, DivergenceErrors, DivergenceHandler, ShadowMut, ShadowRef, ShadowVm,
    },
};

mod dump;
mod shadow;
