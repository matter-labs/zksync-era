use std::rc::Rc;

use zksync_types::{
    commitment::{L1BatchCommitmentMode, PubdataParams},
    l2_to_l1_log::L2ToL1Log,
    writes::StateDiffRecord,
    Address, ProtocolVersionId, U256,
};
use zksync_utils::{u256_to_bytes_be, u256_to_h256};

use crate::pubdata::{rollup::RollupPubdataBuilder, validium::ValidiumPubdataBuilder};

pub mod rollup;
pub mod utils;
pub mod validium;

#[cfg(test)]
mod tests;

/// Corresponds to the following solidity event:
/// ```solidity
/// struct L2ToL1Log {
///     uint8 l2ShardId;
///     bool isService;
///     uint16 txNumberInBlock;
///     address sender;
///     bytes32 key;
///     bytes32 value;
/// }
/// ```
#[derive(Debug, Default, Clone, PartialEq)]
pub struct L1MessengerL2ToL1Log {
    pub l2_shard_id: u8,
    pub is_service: bool,
    pub tx_number_in_block: u16,
    pub sender: Address,
    pub key: U256,
    pub value: U256,
}

impl L1MessengerL2ToL1Log {
    pub fn packed_encoding(&self) -> Vec<u8> {
        let mut res: Vec<u8> = vec![];
        res.push(self.l2_shard_id);
        res.push(self.is_service as u8);
        res.extend_from_slice(&self.tx_number_in_block.to_be_bytes());
        res.extend_from_slice(self.sender.as_bytes());
        res.extend(u256_to_bytes_be(&self.key));
        res.extend(u256_to_bytes_be(&self.value));
        res
    }
}

impl From<L1MessengerL2ToL1Log> for L2ToL1Log {
    fn from(log: L1MessengerL2ToL1Log) -> Self {
        L2ToL1Log {
            shard_id: log.l2_shard_id,
            is_service: log.is_service,
            tx_number_in_block: log.tx_number_in_block,
            sender: log.sender,
            key: u256_to_h256(log.key),
            value: u256_to_h256(log.value),
        }
    }
}

/// Struct based on which the pubdata blob is formed
#[derive(Debug, Clone, Default)]
pub struct PubdataInput {
    pub user_logs: Vec<L1MessengerL2ToL1Log>,
    pub l2_to_l1_messages: Vec<Vec<u8>>,
    pub published_bytecodes: Vec<Vec<u8>>,
    pub state_diffs: Vec<StateDiffRecord>,
}

/// Trait that encapsulates pubdata building logic. It is implemented for rollup and validium cases.
/// If chains needs custom pubdata format then another implementation should be added.
pub trait PubdataBuilder: std::fmt::Debug {
    fn pubdata_params(&self) -> Option<PubdataParams> {
        None
    }

    fn l2_da_validator(&self) -> Address;

    fn l1_messenger_operator_input(
        &self,
        input: PubdataInput,
        protocol_version: ProtocolVersionId,
    ) -> Vec<u8>;

    fn settlement_layer_pubdata(
        &self,
        input: PubdataInput,
        protocol_version: ProtocolVersionId,
    ) -> Vec<u8>;
}

pub fn pubdata_params_to_builder(params: PubdataParams) -> Rc<dyn PubdataBuilder> {
    match params.pubdata_type {
        L1BatchCommitmentMode::Rollup => {
            Rc::new(RollupPubdataBuilder::new(params.l2_da_validator_address))
        }
        L1BatchCommitmentMode::Validium => {
            Rc::new(ValidiumPubdataBuilder::new(params.l2_da_validator_address))
        }
    }
}
