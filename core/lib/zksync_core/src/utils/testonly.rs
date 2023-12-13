//! Test utils.

use zksync_contracts::BaseSystemContractsHashes;
use zksync_types::{block::MiniblockHeader, Address, MiniblockNumber, ProtocolVersionId, H256};

pub(crate) fn create_miniblock(number: u32) -> MiniblockHeader {
    MiniblockHeader {
        number: MiniblockNumber(number),
        timestamp: number.into(),
        hash: H256::from_low_u64_be(number.into()),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: Address::zero(),
        base_fee_per_gas: 100,
        l1_gas_price: 100,
        l2_fair_gas_price: 100,
        base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        virtual_blocks: 1,
    }
}
