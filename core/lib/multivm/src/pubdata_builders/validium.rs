use zksync_types::{
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
pub struct ValidiumPubdataBuilder {
    pub l2_da_validator: Address,
}

impl ValidiumPubdataBuilder {
    pub fn new(l2_da_validator: Address) -> Self {
        Self { l2_da_validator }
    }
}

impl PubdataBuilder for ValidiumPubdataBuilder {
    fn l2_da_validator(&self) -> Address {
        self.l2_da_validator
    }

    fn l1_messenger_operator_input(
        &self,
        input: &PubdataInput,
        protocol_version: ProtocolVersionId,
    ) -> Vec<u8> {
        assert!(
            !protocol_version.is_pre_gateway(),
            "ValidiumPubdataBuilder must not be called for pre gateway"
        );

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

        [func_selector.to_vec(), ethabi::encode(&l2_da_header)]
            .concat()
            .to_vec()
    }

    fn settlement_layer_pubdata(
        &self,
        input: &PubdataInput,
        protocol_version: ProtocolVersionId,
    ) -> Vec<u8> {
        assert!(
            !protocol_version.is_pre_gateway(),
            "ValidiumPubdataBuilder must not be called for pre gateway"
        );

        let mut pubdata = vec![];
        extend_from_pubdata_input(&mut pubdata, input);

        pubdata

        // let state_diffs_packed = input
        //     .state_diffs
        //     .iter()
        //     .flat_map(|diff| diff.encode_padded())
        //     .collect::<Vec<_>>();
        //
        // keccak256(&state_diffs_packed).to_vec()
    }
}
