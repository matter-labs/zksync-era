use assert_cmd::Command;

#[test]
#[doc = "prover_cli"]
fn pli_empty_fails() {
    Command::cargo_bin("prover_cli").unwrap().assert().failure();
}

#[test]
#[doc = "prover_cli"]
fn pli_help_succeeds() {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("help")
        .assert()
        .success();
}
