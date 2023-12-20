use anyhow::Context as _;
use zksync_consensus_roles::validator;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_protobuf::{required, ProtoFmt};
use zksync_types::{
    api::en, Address, L1BatchNumber, MiniblockNumber, ProtocolVersionId, Transaction, H160, H256,
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
    pub bootloader_code_hash: Option<Vec<u8>>,
    pub default_aa_code_hash: Option<Vec<u8>>,
    pub fee_account_address: Option<Vec<u8>>, // May be None if the block is not yet sealed
    pub protocol_version: i32,
    pub virtual_blocks: i64,
    pub hash: Vec<u8>,
}

fn parse_h256(bytes: &[u8]) -> anyhow::Result<H256> {
    Ok(<[u8; 32]>::try_from(bytes).context("invalid size")?.into())
}

fn parse_h160(bytes: &[u8]) -> anyhow::Result<H160> {
    Ok(<[u8; 20]>::try_from(bytes).context("invalid size")?.into())
}

pub(crate) struct SyncBlock {
    pub number: MiniblockNumber,
    pub l1_batch_number: L1BatchNumber,
    pub last_in_batch: bool,
    pub timestamp: u64,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub fee_account_address: Option<Address>,
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
            fee_account_address: block
                .fee_account_address
                .map(|a| parse_h160(&a))
                .transpose()
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
    pub(crate) fn into_api(
        self,
        current_operator_address: Address,
        transactions: Option<Vec<Transaction>>,
    ) -> en::SyncBlock {
        en::SyncBlock {
            number: self.number,
            l1_batch_number: self.l1_batch_number,
            last_in_batch: self.last_in_batch,
            timestamp: self.timestamp,
            l1_gas_price: self.l1_gas_price,
            l2_fair_gas_price: self.l2_fair_gas_price,
            base_system_contracts_hashes: self.base_system_contracts_hashes,
            operator_address: self.fee_account_address.unwrap_or(current_operator_address),
            transactions,
            virtual_blocks: Some(self.virtual_blocks),
            hash: Some(self.hash),
            protocol_version: self.protocol_version,
        }
    }

    pub(crate) fn into_payload(
        self,
        current_operator_address: Address,
        transactions: Vec<Transaction>,
    ) -> Payload {
        Payload {
            protocol_version: self.protocol_version,
            hash: self.hash,
            l1_batch_number: self.l1_batch_number,
            timestamp: self.timestamp,
            l1_gas_price: self.l1_gas_price,
            l2_fair_gas_price: self.l2_fair_gas_price,
            virtual_blocks: self.virtual_blocks,
            operator_address: self.fee_account_address.unwrap_or(current_operator_address),
            transactions,
            last_in_batch: self.last_in_batch,
        }
    }
}

/// L2 block (= miniblock) payload.
#[derive(Debug, PartialEq)]
pub struct Payload {
    pub protocol_version: ProtocolVersionId,
    pub hash: H256,
    pub l1_batch_number: L1BatchNumber,
    pub timestamp: u64,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    pub virtual_blocks: u32,
    pub operator_address: Address,
    pub transactions: Vec<Transaction>,
    pub last_in_batch: bool,
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
                .and_then(|h| parse_h256(h))
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
                .and_then(|a| parse_h160(a))
                .context("operator_address")?,
            transactions,
            last_in_batch: *required(&message.last_in_batch).context("last_in_batch")?,
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
            last_in_batch: Some(self.last_in_batch),
        }
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
