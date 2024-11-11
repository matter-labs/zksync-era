use crate::{vm_latest::MultiVMSubversion, VmVersion};

#[derive(Debug, Copy, Clone)]
pub(crate) enum FastVMVersion {
    IncreasedBootloaderMemory,
    Gateway,
}

impl From<FastVMVersion> for MultiVMSubversion {
    fn from(value: FastVMVersion) -> Self {
        match value {
            FastVMVersion::IncreasedBootloaderMemory => Self::IncreasedBootloaderMemory,
            FastVMVersion::Gateway => Self::Gateway,
        }
    }
}

impl TryFrom<VmVersion> for FastVMVersion {
    type Error = ();

    fn try_from(value: VmVersion) -> Result<Self, Self::Error> {
        match value {
            VmVersion::Vm1_5_0IncreasedBootloaderMemory => Ok(Self::IncreasedBootloaderMemory),
            VmVersion::VmGateway => Ok(Self::Gateway),
            _ => Err(()),
        }
    }
}
