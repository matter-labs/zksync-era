use anyhow::Context as _;
use zksync_consensus_roles::validator;
use zksync_protobuf::{required, ProtoFmt};
use zksync_types::api::en::SyncBlock;
use zksync_types::{Address, L1BatchNumber, Transaction, H256};

pub(crate) struct Payload {
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
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        let mut transactions = vec![];
        for (i, t) in r.transactions.iter().enumerate() {
            transactions.push(
                required(&t.json)
                    .and_then(|s| Ok(serde_json::from_str(&*s)?))
                    .with_context(|| format!("transaction[{i}]"))?,
            );
        }
        Ok(Self {
            hash: required(&r.hash)
                .and_then(|h| Ok(<[u8; 32]>::try_from(h.as_slice())?.into()))
                .context("hash")?,
            l1_batch_number: L1BatchNumber(
                *required(&r.l1_batch_number).context("l1_batch_number")?,
            ),
            timestamp: *required(&r.timestamp).context("timestamp")?,
            l1_gas_price: *required(&r.l1_gas_price).context("l1_gas_price")?,
            l2_fair_gas_price: *required(&r.l2_fair_gas_price).context("l2_fair_gas_price")?,
            virtual_blocks: *required(&r.virtual_blocks).context("virtual_blocks")?,
            operator_address: required(&r.operator_address)
                .and_then(|a| Ok(<[u8; 20]>::try_from(a.as_slice())?.into()))
                .context("operator_address")?,
            transactions,
        })
    }
    fn build(&self) -> Self::Proto {
        Self::Proto {
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
    pub fn decode(p: &validator::Payload) -> anyhow::Result<Self> {
        zksync_protobuf::decode(&p.0)
    }

    pub fn encode(&self) -> validator::Payload {
        validator::Payload(zksync_protobuf::encode(self))
    }
}
