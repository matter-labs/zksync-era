use serde::Deserialize;
use zksync_basic_types::{ethabi, ethabi::ParamType, Address, L2ChainId, H256, U256};
use zksync_web3_decl::{jsonrpsee::core::Serialize, types::Log};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteropCall {
    pub direct_call: bool,
    pub to: Address,
    pub from: Address,
    pub value: U256,
    pub data: Vec<u8>,
}

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

#[derive(Debug, Clone)]
pub struct InteropBundle {
    pub tx_hash: H256,
    // pub src_chain_id: L2ChainId,
    pub dst_chain_id: L2ChainId,
    pub calls: Vec<InteropCall>,
    pub execution_address: Address,
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
        let mut trigger = decoded.remove(1).into_tuple().unwrap();
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
