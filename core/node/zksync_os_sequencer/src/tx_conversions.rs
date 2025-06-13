// we use zksync-era transaction types for now, so we need to convert back and forth.
// we should have a lightweight wrapper for zksync-os (or use some common crate) to avoid this conversion

use zk_os_forward_system::run::BatchOutput;
use zksync_types::{api, H256, U256};
use crate::CHAIN_ID;
use crate::storage::in_memory_tx_receipts::TransactionApiData;

pub fn transaction_to_api_data(
    block_output: &BatchOutput,
    index: usize,
    tx: &zksync_types::Transaction
) -> TransactionApiData {
    // let mut ts = std::time::Instant::now();

    let api_tx = api::Transaction {
        hash: tx.hash(),
        nonce: U256::from(tx.nonce().map(|n| n.0).unwrap_or(0)),
        // block_hash: Some(block_output.header.hash().into()),
        block_hash: Some(H256::default()),
        block_number: Some(block_output.header.number.into()),
        transaction_index: Some(index.into()),
        from: Some(tx.initiator_account()),
        to: tx.execute.contract_address,
        gas_price: Some(U256::from(1)),
        gas: U256::from(100),
        chain_id: U256::from(CHAIN_ID),
        value: tx.execute.value,
        transaction_type: Some((tx.tx_format() as u64).into()),
        input: "0x0000".into(),
        r: Some(U256::zero()),
        v: Some(0.into()),
        s: Some(U256::zero()),
        max_fee_per_gas: Some(U256::from(1)),
        y_parity: None,
        max_priority_fee_per_gas: Some(U256::from(1)),
        //todo: other fields
        .. Default::default()
    };

    // tracing::info!("Block {} - saving - api::Transaction in {:?},", block_output.header.number, ts.elapsed());
    // ts = std::time::Instant::now();


    let api_receipt = api::TransactionReceipt {
        transaction_hash: tx.hash(),
        transaction_index: index.into(),
        // block_hash: block_output.header.hash().into(),
        block_hash: H256::default(),
        block_number: block_output.header.number.into(),
        l1_batch_tx_index: None,
        l1_batch_number: None,
        from: tx.initiator_account(),
        to: tx.execute.contract_address,
        // todo
        cumulative_gas_used: 7777.into(),
        gas_used: Some(100.into()),
        contract_address: None, //todo:
        logs: vec![],
        l2_to_l1_logs: vec![],
        status: 1.into(),
        logs_bloom: Default::default(),
        transaction_type: None,
        effective_gas_price: None,
    };

    // tracing::info!("Block {} - saving - api::TransactionReceipt in {:?},", block_output.header.number, ts.elapsed());
    // ts = std::time::Instant::now();

    TransactionApiData {
        transaction: api_tx,
        receipt: api_receipt,
    }
}
