use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(
    Debug,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    ValueEnum,
    EnumIter,
    strum_macros::Display,
    PartialEq,
    Eq,
)]
pub enum ProverMode {
    NoProofs,
    Gpu,
    Cpu,
}
