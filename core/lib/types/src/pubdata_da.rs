use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};

/// Enum holding the current values used for DA Layers.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
#[derive(TryFromPrimitive)]
pub enum PubdataDA {
    Calldata = 0,
    Blobs,
}
