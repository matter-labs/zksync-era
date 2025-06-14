use crate::{vm_latest::MultiVmSubversion, VmVersion};

#[derive(Debug, Copy, Clone)]
pub(crate) enum FastVmVersion {
    IncreasedBootloaderMemory,
    Gateway,
    Interop,
}

impl From<FastVmVersion> for MultiVmSubversion {
    fn from(value: FastVmVersion) -> Self {
        match value {
            FastVmVersion::IncreasedBootloaderMemory => Self::IncreasedBootloaderMemory,
            FastVmVersion::Gateway => Self::Gateway,
            FastVmVersion::Interop => Self::latest(),
        }
    }
}

impl TryFrom<VmVersion> for FastVmVersion {
    type Error = ();

    fn try_from(value: VmVersion) -> Result<Self, Self::Error> {
        match value {
            VmVersion::Vm1_5_0IncreasedBootloaderMemory => Ok(Self::IncreasedBootloaderMemory),
            // FIXME: implement differentiated memory model in fast VM
            VmVersion::VmGateway | VmVersion::VmEvmEmulator | VmVersion::VmEcPrecompiles => {
                Ok(Self::Gateway)
            }
            VmVersion::VmInterop => Ok(Self::Interop),
            _ => Err(()),
        }
    }
}
