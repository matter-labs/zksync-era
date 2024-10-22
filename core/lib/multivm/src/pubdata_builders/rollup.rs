use zksync_types::{
    ethabi,
    ethabi::{ParamType, Token},
    l2_to_l1_log::l2_to_l1_logs_tree_size,
    writes::compress_state_diffs,
    Address, ProtocolVersionId,
};

use super::utils::{
    build_chained_bytecode_hash, build_chained_log_hash, build_chained_message_hash,
    build_logs_root, encode_user_logs,
};
use crate::interface::pubdata::{PubdataBuilder, PubdataInput};

#[derive(Debug, Clone, Copy)]
pub struct RollupPubdataBuilder {
    pub l2_da_validator: Address,
}

impl RollupPubdataBuilder {
    pub fn new(l2_da_validator: Address) -> Self {
        Self { l2_da_validator }
    }
}

impl PubdataBuilder for RollupPubdataBuilder {
    fn l2_da_validator(&self) -> Address {
        self.l2_da_validator
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

fn extend_from_pubdata_input(buffer: &mut Vec<u8>, pubdata_input: &PubdataInput) {
    let PubdataInput {
        user_logs,
        l2_to_l1_messages,
        published_bytecodes,
        state_diffs,
    } = pubdata_input;

    // Adding user L2->L1 logs.
    buffer.extend(encode_user_logs(user_logs));

    // Encoding L2->L1 messages
    // Format: `[(numberOfMessages as u32) || (messages[1].len() as u32) || messages[1] || ... || (messages[n].len() as u32) || messages[n]]`
    buffer.extend((l2_to_l1_messages.len() as u32).to_be_bytes());
    for message in l2_to_l1_messages {
        buffer.extend((message.len() as u32).to_be_bytes());
        buffer.extend(message);
    }
    // Encoding bytecodes
    // Format: `[(numberOfBytecodes as u32) || (bytecodes[1].len() as u32) || bytecodes[1] || ... || (bytecodes[n].len() as u32) || bytecodes[n]]`
    buffer.extend((published_bytecodes.len() as u32).to_be_bytes());
    for bytecode in published_bytecodes {
        buffer.extend((bytecode.len() as u32).to_be_bytes());
        buffer.extend(bytecode);
    }
    // Encoding state diffs
    // Format: `[size of compressed state diffs u32 || compressed state diffs || (# state diffs: intial + repeated) as u32 || sorted state diffs by <index, address, key>]`
    let state_diffs_compressed = compress_state_diffs(state_diffs.clone());
    buffer.extend(state_diffs_compressed);
}
