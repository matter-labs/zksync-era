#![allow(warnings)]

pub use self::{config::*, core::config::*};

include!(concat!(env!("OUT_DIR"), "/src/proto/gen.rs"));
