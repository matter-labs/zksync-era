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
