use anyhow::Context as _;
use zksync_consensus_roles::validator;
use zksync_protobuf::{required, ProtoFmt};
use zksync_types::{
    api::en::SyncBlock, Address, L1BatchNumber, ProtocolVersionId, Transaction, H256,
};

/// L2 block (= miniblock) payload.
#[derive(Debug, PartialEq)]
pub(crate) struct Payload {
    pub protocol_version: ProtocolVersionId,
    pub hash: H256,
    pub l1_batch_number: L1BatchNumber,
    pub timestamp: u64,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    pub virtual_blocks: u32,
    pub operator_address: Address,
    pub transactions: Vec<Transaction>,
}

impl ProtoFmt for Payload {
    type Proto = super::proto::Payload;

    fn read(message: &Self::Proto) -> anyhow::Result<Self> {
        let mut transactions = Vec::with_capacity(message.transactions.len());
        for (i, tx) in message.transactions.iter().enumerate() {
            transactions.push(
                required(&tx.json)
                    .and_then(|json_str| Ok(serde_json::from_str(json_str)?))
                    .with_context(|| format!("transaction[{i}]"))?,
            );
        }

        Ok(Self {
            protocol_version: required(&message.protocol_version)
                .and_then(|x| Ok(ProtocolVersionId::try_from(u16::try_from(*x)?)?))
                .context("protocol_version")?,
            hash: required(&message.hash)
                .and_then(|bytes| Ok(<[u8; 32]>::try_from(bytes.as_slice())?.into()))
                .context("hash")?,
            l1_batch_number: L1BatchNumber(
                *required(&message.l1_batch_number).context("l1_batch_number")?,
            ),
            timestamp: *required(&message.timestamp).context("timestamp")?,
            l1_gas_price: *required(&message.l1_gas_price).context("l1_gas_price")?,
            l2_fair_gas_price: *required(&message.l2_fair_gas_price)
                .context("l2_fair_gas_price")?,
            virtual_blocks: *required(&message.virtual_blocks).context("virtual_blocks")?,
            operator_address: required(&message.operator_address)
                .and_then(|bytes| Ok(<[u8; 20]>::try_from(bytes.as_slice())?.into()))
                .context("operator_address")?,
            transactions,
        })
    }
    fn build(&self) -> Self::Proto {
        Self::Proto {
            protocol_version: Some((self.protocol_version as u16).into()),
            hash: Some(self.hash.as_bytes().into()),
            l1_batch_number: Some(self.l1_batch_number.0),
            timestamp: Some(self.timestamp),
            l1_gas_price: Some(self.l1_gas_price),
            l2_fair_gas_price: Some(self.l2_fair_gas_price),
            virtual_blocks: Some(self.virtual_blocks),
            operator_address: Some(self.operator_address.as_bytes().into()),
            // Transactions are stored in execution order, therefore order is deterministic.
            transactions: self
                .transactions
                .iter()
                .map(|t| super::proto::Transaction {
                    // TODO: There is no guarantee that json encoding here will be deterministic.
                    json: Some(serde_json::to_string(t).unwrap()),
                })
                .collect(),
        }
    }
}

impl TryFrom<SyncBlock> for Payload {
    type Error = anyhow::Error;

    fn try_from(block: SyncBlock) -> anyhow::Result<Self> {
        Ok(Self {
            protocol_version: block.protocol_version,
            hash: block.hash.unwrap_or_default(),
            l1_batch_number: block.l1_batch_number,
            timestamp: block.timestamp,
            l1_gas_price: block.l1_gas_price,
            l2_fair_gas_price: block.l2_fair_gas_price,
            virtual_blocks: block.virtual_blocks.unwrap_or(0),
            operator_address: block.operator_address,
            transactions: block.transactions.context("Transactions are required")?,
        })
    }
}

impl Payload {
    pub fn decode(payload: &validator::Payload) -> anyhow::Result<Self> {
        zksync_protobuf::decode(&payload.0)
    }

    pub fn encode(&self) -> validator::Payload {
        validator::Payload(zksync_protobuf::encode(self))
    }
}
