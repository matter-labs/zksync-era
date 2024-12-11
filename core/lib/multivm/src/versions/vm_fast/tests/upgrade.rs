use crate::{
    versions::testonly::upgrade::{
        test_complex_upgrader, test_force_deploy_upgrade, test_protocol_upgrade_is_first,
    },
    vm_fast::Vm,
};

#[test]
fn protocol_upgrade_is_first() {
    test_protocol_upgrade_is_first::<Vm<_>>();
}

#[test]
fn force_deploy_upgrade() {
    test_force_deploy_upgrade::<Vm<_>>();
}

#[test]
fn complex_upgrader() {
    test_complex_upgrader::<Vm<_>>();
}
