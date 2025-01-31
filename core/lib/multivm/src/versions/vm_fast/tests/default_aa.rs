use crate::{versions::testonly::default_aa::test_default_aa_interaction, vm_fast::Vm};

#[test]
fn default_aa_interaction() {
    test_default_aa_interaction::<Vm<_>>();
}
