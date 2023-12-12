use anyhow::Context as _;
use zksync_consensus_roles::validator;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_protobuf::{read_required, ProtoFmt};
use zksync_types::{api::en, L1BatchNumber, MiniblockNumber, Transaction, H160, H256};

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
    pub fee_account_address: Vec<u8>,
    pub protocol_version: i32,
    pub virtual_blocks: i64,
    pub hash: Vec<u8>,
    pub consensus: Option<serde_json::Value>,
}

fn parse_h256(bytes: &[u8]) -> anyhow::Result<H256> {
    Ok(<[u8; 32]>::try_from(bytes).context("invalid size")?.into())
}

fn parse_h160(bytes: &[u8]) -> anyhow::Result<H160> {
    Ok(<[u8; 20]>::try_from(bytes).context("invalid size")?.into())
}

impl StorageSyncBlock {
    pub(crate) fn into_sync_block(
        self,
        transactions: Option<Vec<Transaction>>,
    ) -> anyhow::Result<en::SyncBlock> {
        Ok(en::SyncBlock {
            number: MiniblockNumber(self.number.try_into().context("number")?),
            l1_batch_number: L1BatchNumber(
                self.l1_batch_number.try_into().context("l1_batch_number")?,
            ),
            last_in_batch: self
                .last_batch_miniblock
                .map(|n| n == self.number)
                .unwrap_or(false),
            timestamp: self.timestamp.try_into().context("timestamp")?,
            l1_gas_price: self.l1_gas_price.try_into().context("l1_gas_price")?,
            l2_fair_gas_price: self
                .l2_fair_gas_price
                .try_into()
                .context("l2_fair_gas_price")?,
            // TODO (SMA-1635): Make these fields non optional in database
            base_system_contracts_hashes: BaseSystemContractsHashes {
                bootloader: parse_h256(
                    &self
                        .bootloader_code_hash
                        .context("bootloader_code_hash should not be none")?,
                )
                .context("bootloader_code_hash")?,
                default_aa: parse_h256(
                    &self
                        .default_aa_code_hash
                        .context("default_aa_code_hash should not be none")?,
                )
                .context("default_aa_code_hash")?,
            },
            operator_address: parse_h160(&self.fee_account_address)
                .context("fee_account_address")?,
            transactions,
            virtual_blocks: Some(self.virtual_blocks.try_into().context("virtual_blocks")?),
            hash: Some(parse_h256(&self.hash).context("hash")?),
            protocol_version: u16::try_from(self.protocol_version)
                .context("protocol_version")?
                .try_into()
                .context("protocol_version")?,
            consensus: match self.consensus {
                None => None,
                Some(v) => {
                    let v: ConsensusBlockFields =
                        zksync_protobuf::serde::deserialize(v).context("consensus")?;
                    Some(v.encode())
                }
            },
        })
    }
}

/// Consensus-related L2 block (= miniblock) fields.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsensusBlockFields {
    /// Hash of the previous consensus block.
    pub parent: validator::BlockHeaderHash,
    /// Quorum certificate for the block.
    pub justification: validator::CommitQC,
}

impl ConsensusBlockFields {
    pub fn decode(src: &en::ConsensusBlockFields) -> anyhow::Result<Self> {
        zksync_protobuf::decode(&src.0 .0)
    }

    pub fn encode(&self) -> en::ConsensusBlockFields {
        en::ConsensusBlockFields(zksync_protobuf::encode(self).into())
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

#[cfg(test)]
mod tests {
    use rand::Rng;
    use zksync_consensus_roles::validator;

    use super::ConsensusBlockFields;

    #[tokio::test]
    async fn encode_decode() {
        let rng = &mut rand::thread_rng();
        let block = rng.gen::<validator::FinalBlock>();
        let want = ConsensusBlockFields {
            parent: block.header.parent,
            justification: block.justification,
        };
        assert_eq!(want, ConsensusBlockFields::decode(&want.encode()).unwrap());
    }
}
