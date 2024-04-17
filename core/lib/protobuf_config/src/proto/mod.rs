#![allow(warnings)]

pub use self::{config::*, core::*};

include!(concat!(env!("OUT_DIR"), "/src/proto/gen.rs"));
