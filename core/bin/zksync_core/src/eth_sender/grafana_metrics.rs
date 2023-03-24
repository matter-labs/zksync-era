use std::time::Instant;
use zksync_dal::StorageProcessor;
use zksync_types::eth_sender::EthTx;
use zksync_utils::time::seconds_since_epoch;

pub fn track_eth_tx_metrics(connection: &mut StorageProcessor<'_>, l1_stage: &str, tx: &EthTx) {
    let start = Instant::now();
    let stage = format!("l1_{}_{}", l1_stage, tx.tx_type.to_string());

    let blocks = connection.blocks_dal().get_blocks_for_eth_tx_id(tx.id);

    // This should be only the case when some blocks were reverted.
    if blocks.is_empty() {
        vlog::warn!("No blocks were found for eth_tx with id = {}", tx.id);
        return;
    }

    for block in blocks {
        metrics::histogram!(
            "server.block_latency",
            (seconds_since_epoch() - block.timestamp) as f64,
            "stage" => stage.clone()
        );
        metrics::counter!(
            "server.processed_txs",
            block.tx_count() as u64,
            "stage" => stage.clone()
        );
        metrics::counter!(
            "server.processed_l1_txs",
            block.l1_tx_count as u64,
            "stage" => stage.clone()
        );
    }

    metrics::histogram!("server.eth_sender.metrics.latency", start.elapsed());
}
