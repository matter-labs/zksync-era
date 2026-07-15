use std::fs::read_to_string;

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Serialize, Deserialize)]
struct CommitmentTest {
    input: CommitmentInput,
    pass_through_data: L1BatchPassThroughData,
    meta_parameters: L1BatchMetaParameters,
    auxiliary_output: L1BatchAuxiliaryOutput,
    hashes: L1BatchCommitmentHash,
}

fn run_test(test_name: &str) {
    let contents = read_to_string(format!("src/commitment/tests/{test_name}.json")).unwrap();
    let commitment_test: CommitmentTest = serde_json::from_str(&contents).unwrap();

    let commitment = L1BatchCommitment::new(commitment_test.input, true).unwrap();

    assert_eq!(
        commitment.pass_through_data,
        commitment_test.pass_through_data
    );
    assert_eq!(commitment.meta_parameters, commitment_test.meta_parameters);
    assert_eq!(
        commitment.auxiliary_output,
        commitment_test.auxiliary_output
    );
    assert_eq!(commitment.hash().unwrap(), commitment_test.hashes);
}

#[test]
fn pre_boojum() {
    run_test("pre_boojum_test");
}

#[test]
fn post_boojum_1_4_1() {
    run_test("post_boojum_1_4_1_test");
}

#[test]
fn post_boojum_1_4_2() {
    run_test("post_boojum_1_4_2_test");
}

#[test]
fn post_boojum_1_5_0() {
    run_test("post_boojum_1_5_0_test");
}

#[test]
fn post_boojum_1_5_0_with_evm() {
    run_test("post_boojum_1_5_0_test_with_evm");
}

#[test]
fn post_gateway() {
    run_test("post_gateway_test");
}
