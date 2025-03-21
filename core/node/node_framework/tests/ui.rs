// TODO: Currently compiling node_framework requires compiling a ton of dependencies
// which takes >5 minutes, making it not viable for unit testing;
// to be revived once the components implementations are moved outside of the crate
#[test]
#[ignore]
fn ui_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/correct/*.rs");
}

#[test]
#[ignore]
fn ui_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/incorrect/*.rs");
}
