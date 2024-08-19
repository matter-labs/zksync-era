use fflonk::bellman::bn256::Fr;
use fflonk::bellman::{bn256, CurveAffine, Engine, PrimeField, PrimeFieldRepr};
use fflonk::FflonkSnarkVerifierCircuitProof;
use zksync_prover_interface::outputs::L1BatchProofForL1;
use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token, U256};

use crate::{i_executor::structures::StoredBatchInfo, Tokenizable, Tokenize};

/// Input required to encode `proveBatches` call.
#[derive(Debug, Clone)]
pub struct ProveBatches {
    pub prev_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub proofs: Vec<L1BatchProofForL1>,
    pub should_verify: bool,
}

impl Tokenize for &ProveBatches {
    fn into_tokens(self) -> Vec<Token> {
        let prev_l1_batch = StoredBatchInfo::from(&self.prev_l1_batch).into_token();
        let batches_arg = self
            .l1_batches
            .iter()
            .map(|batch| StoredBatchInfo::from(batch).into_token())
            .collect();
        let batches_arg = Token::Array(batches_arg);

        if self.should_verify {
            // currently we only support submitting a single proof
            assert_eq!(self.proofs.len(), 1);
            assert_eq!(self.l1_batches.len(), 1);

            let L1BatchProofForL1 {
                aggregation_result_coords,
                scheduler_proof,
                ..
            } = self.proofs.first().unwrap();


            fn serialize_fe_for_ethereum(field_element: &Fr) -> U256 {
                let mut be_bytes = [0u8; 32];
                field_element
                    .into_repr()
                    .write_be(&mut be_bytes[..])
                    .expect("get new root BE bytes");
                U256::from_big_endian(&be_bytes[..])
            }

            fn serialize_g1_for_ethereum(
                point: &<bn256::Bn256 as Engine>::G1Affine
            ) -> (U256, U256) {
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

            fn serialize_evm(proof: &FflonkSnarkVerifierCircuitProof) -> Vec<U256> {
                let mut serialized_proof = vec![];
                for input in proof.inputs.iter() {
                    serialized_proof.push(serialize_fe_for_ethereum(input));
                }

                for c in proof.commitments.iter() {
                    let (x, y) = serialize_g1_for_ethereum(c);
                    serialized_proof.push(x);
                    serialized_proof.push(y);
                }

                for el in proof.evaluations.iter() {
                    serialized_proof.push(serialize_fe_for_ethereum(el));
                }

                for el in proof.lagrange_basis_inverses.iter() {
                    serialized_proof.push(serialize_fe_for_ethereum(el));
                }

                serialized_proof
            }

            let proof = serialize_evm(scheduler_proof);

            let aggregation_result_coords = if self.l1_batches[0]
                .header
                .protocol_version
                .unwrap()
                .is_pre_boojum()
            {
                Token::Array(
                    aggregation_result_coords
                        .iter()
                        .map(|bytes| Token::Uint(U256::from_big_endian(bytes)))
                        .collect(),
                )
            } else {
                Token::Array(Vec::new())
            };
            let proof_input = Token::Tuple(vec![
                aggregation_result_coords,
                Token::Array(proof.into_iter().map(Token::Uint).collect()),
            ]);

            vec![prev_l1_batch, batches_arg, proof_input]
        } else {
            vec![
                prev_l1_batch,
                batches_arg,
                Token::Tuple(vec![Token::Array(vec![]), Token::Array(vec![])]),
            ]
        }
    }
}
