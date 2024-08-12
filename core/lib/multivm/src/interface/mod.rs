pub(crate) mod traits;

pub use zksync_vm_interface::*;

pub use self::traits::{
    tracers::dyn_tracers,
    vm::{VmFactory, VmInterface, VmInterfaceHistoryEnabled},
}; // FIXME: re-export from root instead
