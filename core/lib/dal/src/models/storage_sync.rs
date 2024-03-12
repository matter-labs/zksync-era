use std::str::FromStr;

use anyhow::Context as _;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_types::{
    api::en, Address, L1BatchNumber, MiniblockNumber, ProtocolVersionId, Transaction, H160, H256,
    U256,
};

use crate::{
    consensus_dal::Payload,
    models::{parse_h160, parse_h256},
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct StorageSyncBlock {
    pub number: i64,
    pub l1_batch_number: i64,
    pub last_batch_miniblock: Option<i64>,
    pub timestamp: i64,
    // L1 gas price assumed in the corresponding batch
    pub l1_gas_price: i64,
    // L2 gas price assumed in the corresponding batch
    pub l2_fair_gas_price: i64,
    pub fair_pubdata_price: Option<i64>,
    pub bootloader_code_hash: Option<Vec<u8>>,
    pub default_aa_code_hash: Option<Vec<u8>>,
    pub fee_account_address: Vec<u8>,
    pub protocol_version: i32,
    pub virtual_blocks: i64,
    pub hash: Vec<u8>,
}

pub(crate) struct SyncBlock {
    pub number: MiniblockNumber,
    pub l1_batch_number: L1BatchNumber,
    pub last_in_batch: bool,
    pub timestamp: u64,
    pub l1_gas_price: U256,
    pub l2_fair_gas_price: U256,
    pub fair_pubdata_price: Option<U256>,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub fee_account_address: Address,
    pub virtual_blocks: u32,
    pub hash: H256,
    pub protocol_version: ProtocolVersionId,
}

impl TryFrom<StorageSyncBlock> for SyncBlock {
    type Error = anyhow::Error;
    fn try_from(block: StorageSyncBlock) -> anyhow::Result<Self> {
        Ok(Self {
            number: MiniblockNumber(block.number.try_into().context("number")?),
            l1_batch_number: L1BatchNumber(
                block
                    .l1_batch_number
                    .try_into()
                    .context("l1_batch_number")?,
            ),
            last_in_batch: block.last_batch_miniblock == Some(block.number),
            timestamp: block.timestamp.try_into().context("timestamp")?,
            l1_gas_price: block.l1_gas_price.try_into().context("l1_gas_price")?,
            l2_fair_gas_price: block
                .l2_fair_gas_price
                .try_into()
                .context("l2_fair_gas_price")?,
            fair_pubdata_price: block
                .fair_pubdata_price
                .map(|v| v.try_into().context("fair_pubdata_price"))
                .transpose()?,
            // TODO (SMA-1635): Make these fields non optional in database
            base_system_contracts_hashes: BaseSystemContractsHashes {
                bootloader: parse_h256(
                    &block
                        .bootloader_code_hash
                        .context("bootloader_code_hash should not be none")?,
                )
                .context("bootloader_code_hash")?,
                default_aa: parse_h256(
                    &block
                        .default_aa_code_hash
                        .context("default_aa_code_hash should not be none")?,
                )
                .context("default_aa_code_hash")?,
            },
            fee_account_address: parse_h160(&block.fee_account_address)
                .context("fee_account_address")?,
            virtual_blocks: block.virtual_blocks.try_into().context("virtual_blocks")?,
            hash: parse_h256(&block.hash).context("hash")?,
            protocol_version: u16::try_from(block.protocol_version)
                .context("protocol_version")?
                .try_into()
                .context("protocol_version")?,
        })
    }
}

impl SyncBlock {
    pub(crate) fn into_api(self, transactions: Option<Vec<Transaction>>) -> en::SyncBlock {
        en::SyncBlock {
            number: self.number,
            l1_batch_number: self.l1_batch_number,
            last_in_batch: self.last_in_batch,
            timestamp: self.timestamp,
            l1_gas_price: self.l1_gas_price,
            l2_fair_gas_price: self.l2_fair_gas_price,
            fair_pubdata_price: self.fair_pubdata_price,
            base_system_contracts_hashes: self.base_system_contracts_hashes,
            operator_address: self.fee_account_address,
            transactions,
            virtual_blocks: Some(self.virtual_blocks),
            hash: Some(self.hash),
            protocol_version: self.protocol_version,
        }
    }

    pub(crate) fn into_payload(self, transactions: Vec<Transaction>) -> Payload {
        Payload {
            protocol_version: self.protocol_version,
            hash: self.hash,
            l1_batch_number: self.l1_batch_number,
            timestamp: self.timestamp,
            l1_gas_price: self.l1_gas_price,
            l2_fair_gas_price: self.l2_fair_gas_price,
            fair_pubdata_price: self.fair_pubdata_price,
            virtual_blocks: self.virtual_blocks,
            operator_address: self.fee_account_address,
            transactions,
            last_in_batch: self.last_in_batch,
        }
    }
}
