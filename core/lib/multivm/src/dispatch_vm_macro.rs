#[macro_export]
macro_rules! dispatch_vm {
    ($self:ident.$function:ident($($params:tt)*)) => {
        match $self {
            VmInstance::VmM5(vm) => vm.$function($($params)*),
            VmInstance::VmM6(vm) => vm.$function($($params)*),
            VmInstance::Vm1_3_2(vm) => vm.$function($($params)*),
            VmInstance::VmVirtualBlocks(vm) => vm.$function($($params)*),
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => vm.$function($($params)*),
            VmInstance::VmBoojumIntegration(vm) => vm.$function($($params)*),
        }
    };
}
