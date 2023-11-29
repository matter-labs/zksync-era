use anyhow::Context as _;
use std::convert::TryInto;

use zksync_consensus_roles::validator;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_protobuf::{read_required, ProtoFmt};
use zksync_types::api::en;
use zksync_types::{Address, L1BatchNumber, MiniblockNumber, Transaction, H256};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageSyncBlock {
    pub number: i64,
    pub l1_batch_number: i64,
    pub last_batch_miniblock: Option<i64>,
    pub timestamp: i64,
    pub root_hash: Option<Vec<u8>>,
    // L1 gas price assumed in the corresponding batch
    pub l1_gas_price: i64,
    // L2 gas price assumed in the corresponding batch
    pub l2_fair_gas_price: i64,
    pub bootloader_code_hash: Option<Vec<u8>>,
    pub default_aa_code_hash: Option<Vec<u8>>,
    pub fee_account_address: Option<Vec<u8>>, // May be None if the block is not yet sealed
    pub protocol_version: i32,
    pub virtual_blocks: i64,
    pub hash: Vec<u8>,
    pub consensus: Option<serde_json::Value>,
}

impl StorageSyncBlock {
    pub(crate) fn into_sync_block(
        self,
        current_operator_address: Address,
        transactions: Option<Vec<Transaction>>,
    ) -> en::SyncBlock {
        let number = self.number;

        en::SyncBlock {
            number: MiniblockNumber(self.number as u32),
            l1_batch_number: L1BatchNumber(self.l1_batch_number as u32),
            last_in_batch: self
                .last_batch_miniblock
                .map(|n| n == number)
                .unwrap_or(false),
            timestamp: self.timestamp as u64,
            root_hash: self.root_hash.as_deref().map(H256::from_slice),
            l1_gas_price: self.l1_gas_price as u64,
            l2_fair_gas_price: self.l2_fair_gas_price as u64,
            // TODO (SMA-1635): Make these filed non optional in database
            base_system_contracts_hashes: BaseSystemContractsHashes {
                bootloader: self
                    .bootloader_code_hash
                    .map(|bootloader_code_hash| H256::from_slice(&bootloader_code_hash))
                    .expect("Should not be none"),
                default_aa: self
                    .default_aa_code_hash
                    .map(|default_aa_code_hash| H256::from_slice(&default_aa_code_hash))
                    .expect("Should not be none"),
            },
            operator_address: self
                .fee_account_address
                .map(|fee_account_address| Address::from_slice(&fee_account_address))
                .unwrap_or(current_operator_address),
            transactions,
            virtual_blocks: Some(self.virtual_blocks as u32),
            hash: Some(H256::from_slice(&self.hash)),
            protocol_version: (self.protocol_version as u16).try_into().unwrap(),
            consensus: self.consensus.map(|v| {
                let v: ConsensusBlockFields = zksync_protobuf::serde::deserialize(v).unwrap();
                v.encode()
            }),
        }
    }
}

/// Consensus-related L2 block (= miniblock) fields.
#[derive(Debug, Clone)]
pub struct ConsensusBlockFields {
    /// Hash of the previous consensus block.
    pub parent: validator::BlockHeaderHash,
    /// Quorum certificate for the block.
    pub justification: validator::CommitQC,
}

impl ConsensusBlockFields {
    pub fn encode(&self) -> en::ConsensusBlockFields {
        en::ConsensusBlockFields(zksync_protobuf::encode(self))
    }
    pub fn decode(x: &en::ConsensusBlockFields) -> anyhow::Result<Self> {
        zksync_protobuf::decode(&x.0)
    }
}

impl ProtoFmt for ConsensusBlockFields {
    type Proto = crate::models::proto::ConsensusBlockFields;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            parent: read_required(&r.parent).context("parent")?,
            justification: read_required(&r.justification).context("justification")?,
        })
    }
    fn build(&self) -> Self::Proto {
        Self::Proto {
            parent: Some(self.parent.build()),
            justification: Some(self.justification.build()),
        }
    }
}
