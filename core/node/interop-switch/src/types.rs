use serde::Deserialize;
use zksync_basic_types::{
    ethabi,
    ethabi::{ParamType, Token},
    Address, L2ChainId, H256, U256,
};
use zksync_web3_decl::{jsonrpsee::core::Serialize, types::Log};

#[derive(Debug, Clone)]
pub struct InteropTrigger {
    pub tx_hash: H256,
    // pub src_chain_id: L2ChainId,
    pub dst_chain_id: L2ChainId,
    pub sender: Address,
    pub recipient: Address,
    pub fee_bundle_hash: H256,
    pub execution_bundle_hash: H256,

    pub gas_limit: U256,
    pub gas_per_pubdata_byte_limit: U256,
    pub refund_recipient: Address,
    pub paymaster: Address,
    pub paymaster_input: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteropCall {
    pub direct_call: bool,
    pub to: Address,
    pub from: Address,
    pub value: U256,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct InteropBundle {
    pub l1_l2_tx_hash: H256,
    pub bundle_hash: H256,
    // pub src_chain_id: L2ChainId,
    pub dst_chain_id: L2ChainId,
    pub calls: Vec<InteropCall>,
    pub execution_address: Address,
}

impl TryFrom<Token> for InteropCall {
    type Error = ethabi::Error;

    fn try_from(token: Token) -> Result<Self, Self::Error> {
        let mut decoded = token.into_tuple().unwrap();
        let direct_call = decoded.remove(0).into_bool().unwrap();
        let to = decoded.remove(0).into_address().unwrap();
        let from = decoded.remove(0).into_address().unwrap();
        let value = decoded.remove(0).into_uint().unwrap();
        let data = decoded.remove(0).into_bytes().unwrap();

        Ok(Self {
            direct_call,
            to,
            from,
            value,
            data,
        })
    }
}

impl TryFrom<Log> for InteropTrigger {
    type Error = ethabi::Error;

    fn try_from(event: Log) -> Result<Self, Self::Error> {
        let trigger_param = ParamType::Tuple(vec![
            ParamType::Uint(256),
            ParamType::Address,
            ParamType::Address,
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
            ParamType::Uint(256),
            ParamType::Uint(256),
            ParamType::Address,
            ParamType::Address,
            ParamType::Bytes,
        ]);

        // Decode data.
        let mut decoded =
            ethabi::decode(&[ParamType::FixedBytes(32), trigger_param], &event.data.0)?;

        let tx_hash = H256::from_slice(&decoded.remove(0).into_fixed_bytes().unwrap()); // Remove the first element (tx_hash)

        // Extract `Trigger` data.
        // todo remove unwraps and return errors
        let mut trigger = decoded.remove(0).into_tuple().unwrap();
        let destination_chain_id = trigger.remove(0).into_uint().unwrap();
        let destination_chain_id =
            L2ChainId::new(destination_chain_id.as_u64()).expect("Invalid chain ID from client");
        let sender = trigger.remove(0).into_address().unwrap();
        let recipient = trigger.remove(0).into_address().unwrap();
        let fee_bundle_hash = H256::from_slice(&trigger.remove(0).into_fixed_bytes().unwrap());
        let execution_bundle_hash =
            H256::from_slice(&trigger.remove(0).into_fixed_bytes().unwrap());
        let gas_limit = trigger.remove(0).into_uint().unwrap();
        let gas_per_pubdata_byte_limit = trigger.remove(0).into_uint().unwrap();
        let refund_recipient = trigger.remove(0).into_address().unwrap();
        let paymaster = trigger.remove(0).into_address().unwrap();
        let paymaster_input = trigger.remove(0).into_bytes().unwrap();

        Ok(Self {
            tx_hash,
            dst_chain_id: destination_chain_id,
            sender,
            recipient,
            fee_bundle_hash,
            execution_bundle_hash,
            gas_limit,
            gas_per_pubdata_byte_limit,
            refund_recipient,
            paymaster,
            paymaster_input,
        })
    }
}

impl TryFrom<Log> for InteropBundle {
    type Error = ethabi::Error;

    fn try_from(event: Log) -> Result<Self, Self::Error> {
        let call_param = ParamType::Tuple(vec![
            ParamType::Bool,
            ParamType::Address,
            ParamType::Address,
            ParamType::Uint(256),
            ParamType::Bytes,
        ]);

        let bundle_param = ParamType::Tuple(vec![
            ParamType::Uint(256),
            ParamType::Array(Box::new(call_param)),
            ParamType::Address,
        ]);

        // Decode data.
        let mut decoded = ethabi::decode(
            &[
                ParamType::FixedBytes(32),
                ParamType::FixedBytes(32),
                bundle_param,
            ],
            &event.data.0,
        )?;

        let l1_l2_tx_hash = H256::from_slice(&decoded.remove(0).into_fixed_bytes().unwrap()); // Remove the first element (tx_hash)
        let bundle_hash = H256::from_slice(&decoded.remove(0).into_fixed_bytes().unwrap()); // Remove the second element (tx_hash)

        // Extract `Bundle` data.
        let mut bundle = decoded.remove(0).into_tuple().unwrap();
        let destination_chain_id = bundle.remove(0).into_uint().unwrap();
        let destination_chain_id =
            L2ChainId::new(destination_chain_id.as_u64()).expect("Invalid chain ID from client");

        let calls = bundle
            .remove(0)
            .into_array()
            .unwrap()
            .iter()
            .map(|call| InteropCall::try_from(call.clone()))
            .collect::<Result<_, _>>()?;
        let execution_address = bundle.remove(0).into_address().unwrap();

        Ok(Self {
            l1_l2_tx_hash,
            bundle_hash,
            dst_chain_id: destination_chain_id,
            calls,
            execution_address,
        })
    }
}

#[cfg(test)]
mod tests {
    use zksync_basic_types::{web3::Bytes, H256, U256, U64};
    use zksync_contracts::interop_center_contract;
    use zksync_system_constants::L2_INTEROP_CENTER_ADDRESS;
    use zksync_web3_decl::types::Log;

    use crate::types::{InteropBundle, InteropTrigger};

    #[test]
    pub fn decode_trigger() {
        let bytes = hex::decode(
            "9b25f657791ab490c1699f1460ca9bd3248d8a855aefe9439ee5aab9f95cf44700000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000068000000000000000000000000e910db7f814378e00c2aee8c84aa3af15aa0c4c5000000000000000000000000000000000000000000000000000000000001000d79a6f9c6134cabd87de0dff0324c755aedd9a7e41de8f72d0e2a16dff6098a0e293198ed9e0f8df0b2b21974dbe8fb1f12dfcdf73b06a02e50061368f3799b6a00000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000001c9c3800000000000000000000000000000000000000000000000000000000000000320000000000000000000000000e910db7f814378e00c2aee8c84aa3af15aa0c4c5000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000"
          ).unwrap();
        let log = Log {
            address: L2_INTEROP_CENTER_ADDRESS,
            topics: vec![interop_center_contract()
                .event("InteropTriggerSent")
                .unwrap()
                .signature()],
            data: Bytes(bytes),
            block_hash: Some(H256::random()),
            block_number: Some(U64::one()),
            l1_batch_number: Some(U64::one()),
            transaction_hash: Some(H256::random()),
            transaction_index: Some(U64::one()),
            log_index: Some(U256::from(18)),
            transaction_log_index: Some(U256::from(18)),
            log_type: None,
            removed: Some(false),
            block_timestamp: Some(U64::from(18)),
        };
        let trigger = InteropTrigger::try_from(log).unwrap();
        dbg!(trigger);
    }

    #[test]
    pub fn decode_bundle() {
        let bytes = hex::decode(
            "59045a7fd138dbd93dc2e9c59b8ebcc660558bde7019b684bdffc90d97ccb7b079a6f9c6134cabd87de0dff0324c755aedd9a7e41de8f72d0e2a16dff6098a0e000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000680000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000001000d000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000d000000000000000000000000e910db7f814378e00c2aee8c84aa3af15aa0c4c500000000000000000000000000000000000000000000000002c68af0bb14000000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000"
        ).unwrap();
        let log = Log {
            address: L2_INTEROP_CENTER_ADDRESS,
            topics: vec![interop_center_contract()
                .event("InteropBundleSent")
                .unwrap()
                .signature()],
            data: Bytes(bytes),
            block_hash: Some(H256::random()),
            block_number: Some(U64::one()),
            l1_batch_number: Some(U64::one()),
            transaction_hash: Some(H256::random()),
            transaction_index: Some(U64::one()),
            log_index: Some(U256::from(18)),
            transaction_log_index: Some(U256::from(18)),
            log_type: None,
            removed: Some(false),
            block_timestamp: Some(U64::from(18)),
        };
        let bundle = InteropBundle::try_from(log).unwrap();
        dbg!(bundle);
        // todo add real checks and not only parsing verifications
    }
}
