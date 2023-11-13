use zksync_consensus_roles::validator;
use zksync_protobuf::{encode, ProtoFmt};
use zksync_types::api::en::SyncBlock;
use zksync_types::{Address, L1BatchNumber, Transaction, H256};

struct Payload {
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
        unimplemented!()
    }
    fn build(&self) -> Self::Proto {
        unimplemented!()
    }
}

pub(crate) fn sync_block_to_payload(block: SyncBlock) -> validator::Payload {
    validator::Payload(encode(Payload {
        hash: block.hash.unwrap_or_default(),
        l1_batch_number: block.l1_batch_number,
        timestamp: block.timestamp,
        l1_gas_price: block.l1_gas_price,
        l2_fair_gas_price: block.l2_fair_gas_price,
        virtual_blocks: block.virtual_blocks.unwrap_or(0),
        operator_address: block.operator_address,
        transactions: block
            .transactions
            .expect("Transactions are always requested"),
    }))
}
