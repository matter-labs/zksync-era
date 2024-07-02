use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, EnumString, Display, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TeeType {
    #[strum(serialize = "sgx")]
    Sgx,
}
