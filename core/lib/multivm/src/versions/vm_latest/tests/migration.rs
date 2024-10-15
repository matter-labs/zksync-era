use crate::{
    versions::testonly::migration::test_migration_for_system_context_aa_interaction,
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn migration_for_system_context_aa_interaction() {
    test_migration_for_system_context_aa_interaction::<Vm<_, HistoryEnabled>>();
}
