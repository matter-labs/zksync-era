use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VMOption {
    #[default]
    EraVM,
    ZKSyncOsVM,
}

impl VMOption {
    pub fn is_zksync_os(&self) -> bool {
        matches!(self, VMOption::ZKSyncOsVM)
    }
}
