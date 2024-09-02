//! Functionality shared among different types of executors.

use vise::{EncodeLabelSet, EncodeLabelValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "interaction", rename_all = "snake_case")]
pub(crate) enum InteractionType {
    Missed,
    GetValue,
    SetValue,
    Total,
}
