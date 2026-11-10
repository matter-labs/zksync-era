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

#[test]
fn airbender_contract_equivalence() {
    let fixture: CommitmentTest =
        serde_json::from_str(include_str!("post_gateway_test.json")).unwrap();
    let mut commitment = L1BatchCommitment::new(fixture.input, true).unwrap();
    let L1BatchAuxiliaryOutput::PostBoojum {
        airbender_aux_commitments,
        ..
    } = &mut commitment.auxiliary_output
    else {
        panic!("post-Boojum fixture")
    };
    *airbender_aux_commitments = Some(AuxCommitments {
        bootloader_initial_content_commitment:
            "0x35d519e586d0b30fb291b1ce24ce8ce0605af7f52c4bad1394febb63e387ed98"
                .parse()
                .unwrap(),
        events_queue_commitment: H256::zero(),
    });
    assert_eq!(commitment.hash().unwrap(), fixture.hashes);
    let artifacts = commitment.airbender_artifacts().unwrap().unwrap();
    // era-contracts#2451, AirbenderCommitmentEquivalence.t.sol.
    assert_eq!(
        artifacts.commitment,
        "0xeb414bd21d1e5e39d8e169b9142b7e7eac72a3ebc5f2c6c43b85ca2f0f7272c7"
            .parse()
            .unwrap()
    );
    assert_ne!(artifacts.commitment, fixture.hashes.commitment);
    for heap in [false, true] {
        let mut changed = commitment.clone();
        let L1BatchAuxiliaryOutput::PostBoojum {
            aux_commitments, ..
        } = &mut changed.auxiliary_output
        else {
            unreachable!()
        };
        if heap {
            aux_commitments.bootloader_initial_content_commitment = H256::repeat_byte(9);
        } else {
            aux_commitments.events_queue_commitment = H256::repeat_byte(9);
        }
        assert_ne!(changed.hash().unwrap(), fixture.hashes);
        assert_eq!(changed.airbender_artifacts().unwrap().unwrap(), artifacts);
    }
}
