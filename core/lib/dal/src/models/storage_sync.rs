use std::str::FromStr;

use zksync_contracts::BaseSystemContractsHashes;
use zksync_db_connection::error::SqlxContext;
use zksync_types::{
    api::en,
    commitment::{L2DACommitmentScheme, PubdataParams, PubdataType},
    parse_h160, parse_h256, parse_h256_opt, Address, InteropRoot, L1BatchNumber, L2BlockNumber,
    ProtocolVersionId, Transaction, H256,
};

use crate::{consensus_dal::Payload, models::parse_protocol_version};

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct StorageSyncBlock {
    pub number: i64,
    pub l1_batch_number: i64,
    pub tx_count: i32,
    pub timestamp: i64,
    // L1 gas price assumed in the corresponding batch
    pub l1_gas_price: i64,
    // L2 gas price assumed in the corresponding batch
    pub l2_fair_gas_price: i64,
    pub fair_pubdata_price: Option<i64>,
    pub bootloader_code_hash: Option<Vec<u8>>,
    pub default_aa_code_hash: Option<Vec<u8>>,
    pub evm_emulator_code_hash: Option<Vec<u8>>,
    pub fee_account_address: Vec<u8>,
    pub protocol_version: i32,
    pub virtual_blocks: i64,
    pub hash: Vec<u8>,
    pub l2_da_validator_address: Option<Vec<u8>>,
    pub l2_da_commitment_scheme: Option<i32>,
    pub pubdata_type: String,
    pub pubdata_limit: Option<i64>,
}

pub(crate) struct SyncBlock {
    pub number: L2BlockNumber,
    pub l1_batch_number: L1BatchNumber,
    pub last_in_batch: bool,
    pub timestamp: u64,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    pub fair_pubdata_price: Option<u64>,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub fee_account_address: Address,
    pub virtual_blocks: u32,
    pub hash: H256,
    pub protocol_version: ProtocolVersionId,
    pub pubdata_params: PubdataParams,
    pub pubdata_limit: Option<u64>,
    pub interop_roots: Vec<InteropRoot>,
}

impl SyncBlock {
    pub(crate) fn new(
        block: StorageSyncBlock,
        interop_roots: Vec<InteropRoot>,
    ) -> Result<Self, sqlx::Error> {
        Ok(Self {
            number: L2BlockNumber(block.number.try_into().decode_column("number")?),
            l1_batch_number: L1BatchNumber(
                block
                    .l1_batch_number
                    .try_into()
                    .decode_column("l1_batch_number")?,
            ),
            last_in_batch: block.tx_count == 0,
            timestamp: block.timestamp.try_into().decode_column("timestamp")?,
            l1_gas_price: block
                .l1_gas_price
                .try_into()
                .decode_column("l1_gas_price")?,
            l2_fair_gas_price: block
                .l2_fair_gas_price
                .try_into()
                .decode_column("l2_fair_gas_price")?,
            fair_pubdata_price: block
                .fair_pubdata_price
                .map(|v| v.try_into().decode_column("fair_pubdata_price"))
                .transpose()?,
            // TODO (SMA-1635): Make these fields non optional in database
            base_system_contracts_hashes: BaseSystemContractsHashes {
                bootloader: parse_h256_opt(block.bootloader_code_hash.as_deref())
                    .decode_column("bootloader_code_hash")?,
                default_aa: parse_h256_opt(block.default_aa_code_hash.as_deref())
                    .decode_column("default_aa_code_hash")?,
                evm_emulator: block
                    .evm_emulator_code_hash
                    .as_deref()
                    .map(parse_h256)
                    .transpose()
                    .decode_column("evm_emulator_code_hash")?,
            },
            fee_account_address: parse_h160(&block.fee_account_address)
                .decode_column("fee_account_address")?,
            virtual_blocks: block
                .virtual_blocks
                .try_into()
                .decode_column("virtual_blocks")?,
            hash: parse_h256(&block.hash).decode_column("hash")?,
            protocol_version: parse_protocol_version(block.protocol_version)?,
            pubdata_params: PubdataParams {
                pubdata_type: PubdataType::from_str(&block.pubdata_type)
                    .decode_column("Invalid pubdata type")?,
                l2_da_commitment_scheme: block
                    .l2_da_commitment_scheme
                    .map(|a| L2DACommitmentScheme::from(a as u8)),
                l2_da_validator_address: block
                    .l2_da_validator_address
                    .map(|a| parse_h160(&a).decode_column("l2_da_validator_address"))
                    .transpose()?,
            },
            pubdata_limit: block.pubdata_limit.map(|l| l as u64),
            interop_roots,
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
            pubdata_params: Some(self.pubdata_params),
            pubdata_limit: self.pubdata_limit,
            interop_roots: Some(self.interop_roots),
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
            pubdata_params: self.pubdata_params,
            pubdata_limit: self.pubdata_limit,
            interop_roots: self.interop_roots,
        }
    }
}
