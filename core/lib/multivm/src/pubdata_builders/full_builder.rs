use zksync_types::{
    commitment::{L2DACommitmentScheme, L2PubdataValidator},
    ethabi,
    ethabi::{ParamType, Token},
    l2_to_l1_log::l2_to_l1_logs_tree_size,
    Address, ProtocolVersionId,
};

use super::utils::{
    build_chained_bytecode_hash, build_chained_log_hash, build_chained_message_hash,
    build_logs_root, extend_from_pubdata_input,
};
use crate::interface::pubdata::{PubdataBuilder, PubdataInput};

#[derive(Debug, Clone, Copy)]
pub struct FullPubdataBuilder {
    l2_pubdata_validator: L2PubdataValidator,
}

impl FullPubdataBuilder {
    pub fn new(l2_pubdata_validator: L2PubdataValidator) -> Self {
        Self {
            l2_pubdata_validator,
        }
    }
}

impl PubdataBuilder for FullPubdataBuilder {
    fn l2_da_validator(&self) -> Option<Address> {
        self.l2_pubdata_validator.l2_da_validator()
    }

    fn l2_da_commitment_scheme(&self) -> Option<L2DACommitmentScheme> {
        self.l2_pubdata_validator.l2_da_commitment_scheme()
    }

    fn l1_messenger_operator_input(
        &self,
        input: &PubdataInput,
        protocol_version: ProtocolVersionId,
    ) -> Vec<u8> {
        if protocol_version.is_pre_gateway() {
            let mut operator_input = vec![];
            extend_from_pubdata_input(&mut operator_input, input);

            // Extend with uncompressed state diffs.
            operator_input.extend((input.state_diffs.len() as u32).to_be_bytes());
            for state_diff in &input.state_diffs {
                operator_input.extend(state_diff.encode_padded());
            }

            operator_input
        } else {
            let mut pubdata = vec![];
            extend_from_pubdata_input(&mut pubdata, input);

            // Extend with uncompressed state diffs.
            pubdata.extend((input.state_diffs.len() as u32).to_be_bytes());
            for state_diff in &input.state_diffs {
                pubdata.extend(state_diff.encode_padded());
            }

            let chained_log_hash = build_chained_log_hash(&input.user_logs);
            let log_root_hash =
                build_logs_root(&input.user_logs, l2_to_l1_logs_tree_size(protocol_version));
            let chained_msg_hash = build_chained_message_hash(&input.l2_to_l1_messages);
            let chained_bytecodes_hash = build_chained_bytecode_hash(&input.published_bytecodes);

            let l2_da_header = vec![
                Token::FixedBytes(chained_log_hash),
                Token::FixedBytes(log_root_hash),
                Token::FixedBytes(chained_msg_hash),
                Token::FixedBytes(chained_bytecodes_hash),
                Token::Bytes(pubdata),
            ];

            // Selector of `IL2DAValidator::validatePubdata`.
            let func_selector = ethabi::short_signature(
                "validatePubdata",
                &[
                    ParamType::FixedBytes(32),
                    ParamType::FixedBytes(32),
                    ParamType::FixedBytes(32),
                    ParamType::FixedBytes(32),
                    ParamType::Bytes,
                ],
            );

            [func_selector.to_vec(), ethabi::encode(&l2_da_header)].concat()
        }
    }

    fn settlement_layer_pubdata(
        &self,
        input: &PubdataInput,
        _protocol_version: ProtocolVersionId,
    ) -> Vec<u8> {
        let mut pubdata = vec![];
        extend_from_pubdata_input(&mut pubdata, input);

        pubdata
    }
}
