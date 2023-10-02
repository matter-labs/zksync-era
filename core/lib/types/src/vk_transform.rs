use crate::{ethabi::Token, H256};
use std::str::FromStr;
use zkevm_test_harness::{
    abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit,
    bellman::{
        bn256::{Bn256, Fq, Fr, G1Affine},
        plonk::better_better_cs::setup::VerificationKey,
        CurveAffine, PrimeField,
    },
    ff::to_hex,
    witness::{
        oracle::VmWitnessOracle,
        recursive_aggregation::{compute_vk_encoding_and_committment, erase_vk_type},
    },
};

/// Calculates commitment for vk from L1 verifier contract.
pub fn l1_vk_commitment(token: Token) -> H256 {
    let vk = vk_from_token(token);
    generate_vk_commitment(vk)
}

pub fn generate_vk_commitment(
    vk: VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
) -> H256 {
    let (_, scheduler_vk_commitment) = compute_vk_encoding_and_committment(erase_vk_type(vk));
    let scheduler_commitment_hex = format!("0x{}", to_hex(&scheduler_vk_commitment));
    H256::from_str(&scheduler_commitment_hex).expect("invalid scheduler commitment")
}

fn vk_from_token(
    vk_token: Token,
) -> VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>> {
    let tokens = unwrap_tuple(vk_token);

    // Sets only fields of `VerificationKey` struct that are needed for computing commitment.
    let mut vk = VerificationKey::empty();
    vk.n = tokens[0].clone().into_uint().unwrap().as_usize();
    vk.num_inputs = tokens[1].clone().into_uint().unwrap().as_usize();
    vk.gate_selectors_commitments = tokens[3]
        .clone()
        .into_fixed_array()
        .unwrap()
        .into_iter()
        .map(g1_affine_from_token)
        .collect();
    vk.gate_setup_commitments = tokens[4]
        .clone()
        .into_fixed_array()
        .unwrap()
        .into_iter()
        .map(g1_affine_from_token)
        .collect();
    vk.permutation_commitments = tokens[5]
        .clone()
        .into_fixed_array()
        .unwrap()
        .into_iter()
        .map(g1_affine_from_token)
        .collect();
    vk.lookup_selector_commitment = Some(g1_affine_from_token(tokens[6].clone()));
    vk.lookup_tables_commitments = tokens[7]
        .clone()
        .into_fixed_array()
        .unwrap()
        .into_iter()
        .map(g1_affine_from_token)
        .collect();
    vk.lookup_table_type_commitment = Some(g1_affine_from_token(tokens[8].clone()));
    vk.non_residues = tokens[9]
        .clone()
        .into_fixed_array()
        .unwrap()
        .into_iter()
        .map(fr_from_token)
        .collect();

    vk
}

fn g1_affine_from_token(token: Token) -> G1Affine {
    let tokens = unwrap_tuple(token);
    G1Affine::from_xy_unchecked(
        Fq::from_str(&tokens[0].clone().into_uint().unwrap().to_string()).unwrap(),
        Fq::from_str(&tokens[1].clone().into_uint().unwrap().to_string()).unwrap(),
    )
}

fn fr_from_token(token: Token) -> Fr {
    let tokens = unwrap_tuple(token);
    Fr::from_str(&tokens[0].clone().into_uint().unwrap().to_string()).unwrap()
}

fn unwrap_tuple(token: Token) -> Vec<Token> {
    if let Token::Tuple(tokens) = token {
        tokens
    } else {
        panic!("Tuple was expected, got: {}", token);
    }
}
