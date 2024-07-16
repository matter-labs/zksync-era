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
    Default,
    PartialEq,
    Eq,
)]
pub enum L1BatchCommitDataGeneratorMode {
    #[default]
    Rollup,
    Validium,
}
