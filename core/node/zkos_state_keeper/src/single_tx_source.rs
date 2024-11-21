use tracing::log::logger;
use zk_os_forward_system::run::{NextTxResponse, TxSource};
use zksync_types::Transaction;
use zksync_zkos_vm_runner::zkos_conversions::tx_abi_encode;

pub struct SingleTxSource {
    pending_response: NextTxResponse,
}

impl SingleTxSource {
    pub fn new(tx: Transaction) -> Self {
        let encoded = tx_abi_encode(tx);
        tracing::info!("Encoded transaction: {:?}", encoded);
        Self {
            pending_response: NextTxResponse::Tx(encoded),
        }
    }
}

impl TxSource for SingleTxSource {
    // Scrappy temporary code
    fn get_next_tx(&mut self) -> NextTxResponse {
        match std::mem::replace(&mut self.pending_response, NextTxResponse::SealBatch) {
            NextTxResponse::Tx(tx) => NextTxResponse::Tx(tx), // Return the Tx the first time
            NextTxResponse::SealBatch => NextTxResponse::SealBatch, // Keep returning SealBatch afterward
        }
    }
}