use crate::{
    versions::testonly::default_aa::{test_default_aa_interaction, test_permissive_aa_works},
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn default_aa_interaction() {
    test_default_aa_interaction::<Vm<_, HistoryEnabled>>();
}

#[test]
fn permissive_aa_works() {
    test_permissive_aa_works::<Vm<_, HistoryEnabled>>();
}
