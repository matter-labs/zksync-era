use std::time::Instant;

use zksync_dal::StorageProcessor;
use zksync_types::eth_sender::EthTx;
use zksync_utils::time::seconds_since_epoch;

pub async fn track_eth_tx_metrics(
    connection: &mut StorageProcessor<'_>,
    l1_stage: &str,
    tx: &EthTx,
) {
    let start = Instant::now();
    let stage = format!("l1_{l1_stage}_{}", tx.tx_type);

    let l1_batch_headers = connection
        .blocks_dal()
        .get_l1_batches_for_eth_tx_id(tx.id)
        .await;

    // This should be only the case when some blocks were reverted.
    if l1_batch_headers.is_empty() {
        vlog::warn!("No L1 batches were found for eth_tx with id = {}", tx.id);
        return;
    }

    for header in l1_batch_headers {
        metrics::histogram!(
            "server.block_latency",
            (seconds_since_epoch() - header.timestamp) as f64,
            "stage" => stage.clone()
        );
        metrics::counter!(
            "server.processed_txs",
            header.tx_count() as u64,
            "stage" => stage.clone()
        );
        metrics::counter!(
            "server.processed_l1_txs",
            header.l1_tx_count as u64,
            "stage" => stage.clone()
        );
    }

    metrics::histogram!("server.eth_sender.metrics.latency", start.elapsed());
}
