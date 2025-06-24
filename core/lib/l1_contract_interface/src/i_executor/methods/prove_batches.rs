use bellman::plonk::better_better_cs::proof::Proof as PlonkProof;
use circuit_definitions::circuit_definitions::aux_layer::{
    ZkSyncSnarkWrapperCircuit, ZkSyncSnarkWrapperCircuitNoLookupCustomGate,
};
use crypto_codegen::serialize_proof;
use fflonk::{
    bellman::{
        bn256,
        bn256::{Bn256, Fr},
        CurveAffine, Engine, Field, PrimeField, PrimeFieldRepr,
    },
    FflonkProof,
};
use zk_os_basic_system::system_implementation::system::BatchOutput;
use zksync_types::{
    commitment::{L1BatchWithMetadata, ZkosCommitment},
    ethabi::{encode, Token},
    L2ChainId, U256,
};

use crate::{
    i_executor::structures::{
        CommitBoojumOSBatchInfo, StoredBatchInfo, SUPPORTED_ENCODING_VERSION,
    },
    zkos_commitment_to_vm_batch_output, Tokenizable, Tokenize,
};

/// Input required to encode `proveBatches` call.
#[derive(Debug, Clone)]
pub struct ProveBatches {
    pub prev_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub proofs: Vec<PlonkProof<Bn256, ZkSyncSnarkWrapperCircuit>>,
    pub should_verify: bool,
}

impl ProveBatches {
    pub fn conditional_into_tokens(
        &self,
        is_verifier_pre_fflonk: bool,
        l2chain_id: L2ChainId,
    ) -> Vec<Token> {
        let batch_commitment = ZkosCommitment::new(&self.prev_l1_batch, l2chain_id);
        let batch_output = zkos_commitment_to_vm_batch_output(&batch_commitment);
        let prev_l1_batch_info =
            StoredBatchInfo::new(&batch_commitment, batch_output.hash()).into_token();

        let batches_arg = self
            .l1_batches
            .iter()
            .map(|batch| {
                let batch_commitment = ZkosCommitment::new(batch, l2chain_id);
                let batch_output = zkos_commitment_to_vm_batch_output(&batch_commitment);
                StoredBatchInfo::new(&batch_commitment, batch_output.hash()).into_token()
            })
            .collect();
        let batches_arg = Token::Array(batches_arg);
        let protocol_version = self.l1_batches[0].header.protocol_version.unwrap();

        if self.should_verify {
            // currently we only support submitting a single proof
            assert_eq!(self.proofs.len(), 1);
            assert_eq!(self.l1_batches.len(), 1);

            let verifier_type = U256::from(1); //plonk
            let (_, proof) = serialize_proof(self.proofs.first().unwrap());

            // original logic:
            // let (verifier_type, proof) = match self.proofs.first().unwrap().inner() {
            //     TypedL1BatchProofForL1::Fflonk(proof) => {
            //         let scheduler_proof = proof.scheduler_proof.clone();
            //
            //         let (_, serialized_proof) = serialize_fflonk_proof(&scheduler_proof);
            //         (U256::from(0), serialized_proof)
            //     }
            //     TypedL1BatchProofForL1::Plonk(proof) => {
            //         let (_, serialized_proof) = serialize_proof(&proof.scheduler_proof);
            //         (U256::from(1), serialized_proof)
            //     }
            // };

            let should_use_fflonk = !is_verifier_pre_fflonk || !protocol_version.is_pre_fflonk();
            assert!(!should_use_fflonk);
            if protocol_version.is_pre_gateway() {
                unreachable!("");
                let proof_input = if should_use_fflonk {
                    Token::Tuple(vec![
                        Token::Array(vec![verifier_type.into_token()]),
                        Token::Array(proof.into_iter().map(Token::Uint).collect()),
                    ])
                } else {
                    Token::Tuple(vec![
                        Token::Array(vec![]),
                        Token::Array(proof.into_iter().map(Token::Uint).collect()),
                    ])
                };

                vec![prev_l1_batch_info, batches_arg, proof_input]
            } else {
                let proof_input = if should_use_fflonk {
                    Token::Array(
                        vec![verifier_type]
                            .into_iter()
                            .chain(proof)
                            .map(Token::Uint)
                            .collect(),
                    )
                } else {
                    Token::Array(proof.into_iter().map(Token::Uint).collect())
                };

                let encoded_data = encode(&[prev_l1_batch_info, batches_arg, proof_input]);
                let prove_data = [[SUPPORTED_ENCODING_VERSION].to_vec(), encoded_data]
                    .concat()
                    .to_vec();

                vec![
                    Token::Uint((self.prev_l1_batch.header.number.0 + 1).into()),
                    Token::Uint(
                        (self.prev_l1_batch.header.number.0 + self.l1_batches.len() as u32).into(),
                    ),
                    Token::Bytes(prove_data),
                ]
            }
        } else if protocol_version.is_pre_gateway() {
            vec![
                prev_l1_batch_info,
                batches_arg,
                Token::Tuple(vec![Token::Array(vec![]), Token::Array(vec![])]),
            ]
        } else {
            let encoded_data = encode(&[prev_l1_batch_info, batches_arg, Token::Array(vec![])]);
            let prove_data = [[SUPPORTED_ENCODING_VERSION].to_vec(), encoded_data]
                .concat()
                .to_vec();

            vec![
                Token::Uint((self.prev_l1_batch.header.number.0 + 1).into()),
                Token::Uint(
                    (self.prev_l1_batch.header.number.0 + self.l1_batches.len() as u32).into(),
                ),
                Token::Bytes(prove_data),
            ]
        }
    }
}

fn serialize_fe_for_ethereum(field_element: &Fr) -> U256 {
    let mut be_bytes = [0u8; 32];
    field_element
        .into_repr()
        .write_be(&mut be_bytes[..])
        .expect("get new root BE bytes");
    U256::from_big_endian(&be_bytes[..])
}

fn serialize_g1_for_ethereum(point: &<bn256::Bn256 as Engine>::G1Affine) -> (U256, U256) {
    if <<bn256::Bn256 as Engine>::G1Affine as CurveAffine>::is_zero(point) {
        return (U256::zero(), U256::zero());
    }
    let (x, y) = <<bn256::Bn256 as Engine>::G1Affine as CurveAffine>::into_xy_unchecked(*point);
    let _ = <<bn256::Bn256 as Engine>::G1Affine as CurveAffine>::from_xy_checked(x, y).unwrap();

    let mut buffer = [0u8; 32];
    x.into_repr().write_be(&mut buffer[..]).unwrap();
    let x = U256::from_big_endian(&buffer);

    let mut buffer = [0u8; 32];
    y.into_repr().write_be(&mut buffer[..]).unwrap();
    let y = U256::from_big_endian(&buffer);

    (x, y)
}

fn serialize_fflonk_proof(
    proof: &FflonkProof<Bn256, ZkSyncSnarkWrapperCircuitNoLookupCustomGate>,
) -> (Vec<U256>, Vec<U256>) {
    let mut serialized_inputs = vec![];
    for input in proof.inputs.iter() {
        serialized_inputs.push(serialize_fe_for_ethereum(input));
    }

    let mut serialized_proof = vec![];

    assert_eq!(proof.commitments.len(), 4);

    for c in proof.commitments.iter() {
        let (x, y) = serialize_g1_for_ethereum(c);
        serialized_proof.push(x);
        serialized_proof.push(y);
    }

    assert_eq!(proof.evaluations.len(), 15);

    for el in proof.evaluations.iter() {
        serialized_proof.push(serialize_fe_for_ethereum(el));
    }

    assert_eq!(proof.lagrange_basis_inverses.len(), 18);

    let mut product = proof.lagrange_basis_inverses[0];

    for i in 1..proof.lagrange_basis_inverses.len() {
        product.mul_assign(&proof.lagrange_basis_inverses[i]);
    }

    serialized_proof.push(serialize_fe_for_ethereum(&product));

    (serialized_inputs, serialized_proof)
}
