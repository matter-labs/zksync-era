use crate::{
    versions::testonly::default_aa::test_default_aa_interaction,
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn default_aa_interaction() {
    test_default_aa_interaction::<Vm<_, HistoryEnabled>>();
}
