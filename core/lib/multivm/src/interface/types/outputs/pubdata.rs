use zksync_types::ethabi;
use zksync_types::{
    event::L1MessengerL2ToL1Log,
    writes::{compress_state_diffs, StateDiffRecord},
};

/// Struct based on which the pubdata blob is formed
#[derive(Debug, Clone, Default)]
pub(crate) struct PubdataInput {
    pub(crate) user_logs: Vec<L1MessengerL2ToL1Log>,
    pub(crate) l2_to_l1_messages: Vec<Vec<u8>>,
    pub(crate) published_bytecodes: Vec<Vec<u8>>,
    pub(crate) state_diffs: Vec<StateDiffRecord>,
}

impl PubdataInput {
    pub(crate) fn build_pubdata(self) -> Vec<u8> {
        let mut l1_messenger_pubdata = vec![];

        let PubdataInput {
            user_logs,
            l2_to_l1_messages,
            published_bytecodes,
            state_diffs,
        } = self;

        // Encoding user L2->L1 logs.
        // Format: [(numberOfL2ToL1Logs as u32) || l2tol1logs[1] || ... || l2tol1logs[n]]
        l1_messenger_pubdata.extend((user_logs.len() as u32).to_be_bytes());
        for l2tol1log in user_logs {
            l1_messenger_pubdata.extend(l2tol1log.packed_encoding());
        }

        // Encoding L2->L1 messages
        // Format: [(numberOfMessages as u32) || (messages[1].len() as u32) || messages[1] || ... || (messages[n].len() as u32) || messages[n]]
        l1_messenger_pubdata.extend((l2_to_l1_messages.len() as u32).to_be_bytes());
        for message in l2_to_l1_messages {
            l1_messenger_pubdata.extend((message.len() as u32).to_be_bytes());
            l1_messenger_pubdata.extend(message);
        }

        // Encoding bytecodes
        // Format: [(numberOfBytecodes as u32) || (bytecodes[1].len() as u32) || bytecodes[1] || ... || (bytecodes[n].len() as u32) || bytecodes[n]]
        l1_messenger_pubdata.extend((published_bytecodes.len() as u32).to_be_bytes());
        for bytecode in published_bytecodes {
            l1_messenger_pubdata.extend((bytecode.len() as u32).to_be_bytes());
            l1_messenger_pubdata.extend(bytecode);
        }

        // Encoding state diffs
        // Format: [size of compressed state diffs u32 || compressed state diffs || (# state diffs: intial + repeated) as u32 || sorted state diffs by <index, address, key>]
        let state_diffs_compressed = compress_state_diffs(state_diffs.clone());
        l1_messenger_pubdata.extend(state_diffs_compressed);
        l1_messenger_pubdata.extend((state_diffs.len() as u32).to_be_bytes());

        for state_diff in state_diffs {
            l1_messenger_pubdata.extend(state_diff.encode_padded());
        }

        // ABI-encoding the final pubdata
        let l1_messenger_abi_encoded_pubdata =
            ethabi::encode(&[ethabi::Token::Bytes(l1_messenger_pubdata)]);

        assert!(
            l1_messenger_abi_encoded_pubdata.len() % 32 == 0,
            "abi encoded bytes array length should be divisible by 32"
        );

        // Need to skip first word as it represents array offset
        // while bootloader expects only [len || data]
        l1_messenger_abi_encoded_pubdata[32..].to_vec()
    }
}
