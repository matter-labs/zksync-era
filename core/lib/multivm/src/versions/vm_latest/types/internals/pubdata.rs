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

#[cfg(test)]
mod tests {
    use zksync_system_constants::{ACCOUNT_CODE_STORAGE_ADDRESS, BOOTLOADER_ADDRESS};
    use zksync_utils::u256_to_h256;

    use super::*;

    #[test]
    fn test_basic_pubdata_building() {
        // Just using some constant addresses for tests
        let addr1 = BOOTLOADER_ADDRESS;
        let addr2 = ACCOUNT_CODE_STORAGE_ADDRESS;

        let user_logs = vec![L1MessengerL2ToL1Log {
            l2_shard_id: 0,
            is_service: false,
            tx_number_in_block: 0,
            sender: addr1,
            key: 1.into(),
            value: 128.into(),
        }];

        let l2_to_l1_messages = vec![hex::decode("deadbeef").unwrap()];

        let published_bytecodes = vec![hex::decode("aaaabbbb").unwrap()];

        // For covering more cases, we have two state diffs:
        // One with enumeration index present (and so it is a repeated write) and the one without it.
        let state_diffs = vec![
            StateDiffRecord {
                address: addr2,
                key: 155.into(),
                derived_key: u256_to_h256(125.into()).0,
                enumeration_index: 12,
                initial_value: 11.into(),
                final_value: 12.into(),
            },
            StateDiffRecord {
                address: addr2,
                key: 156.into(),
                derived_key: u256_to_h256(126.into()).0,
                enumeration_index: 0,
                initial_value: 0.into(),
                final_value: 14.into(),
            },
        ];

        let input = PubdataInput {
            user_logs,
            l2_to_l1_messages,
            published_bytecodes,
            state_diffs,
        };

        let pubdata = input.build_pubdata();

        assert_eq!(hex::encode(pubdata), "00000000000000000000000000000000000000000000000000000000000002c700000001000000000000000000000000000000000000000000008001000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000100000004deadbeef0000000100000004aaaabbbb0100002a040001000000000000000000000000000000000000000000000000000000000000007e090e0000000c0901000000020000000000000000000000000000000000008002000000000000000000000000000000000000000000000000000000000000009b000000000000000000000000000000000000000000000000000000000000007d000000000000000c000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008002000000000000000000000000000000000000000000000000000000000000009c000000000000000000000000000000000000000000000000000000000000007e00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000");
    }
}
