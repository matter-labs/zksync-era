use crate::api_server::web3::{backend_jsonrpc::error::internal_error, RpcState};
use crate::l1_gas_price::L1GasPriceProvider;
use zksync_types::{
    api::{flat_call, OpenEthActionTrace},
    H256,
};

#[derive(Debug)]
pub struct TraceNamespace<G> {
    pub state: RpcState<G>,
}
impl<G> Clone for TraceNamespace<G> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<G: L1GasPriceProvider> TraceNamespace<G> {
    pub fn new(state: RpcState<G>) -> Self {
        Self { state }
    }

    #[tracing::instrument(skip(self))]
    pub async fn trace_transaction_impl(&self, tx_hash: H256) -> Vec<OpenEthActionTrace> {
        let mut transaction = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .transactions_web3_dal()
            .get_transaction(tx_hash.into(), self.state.api_config.l2_chain_id)
            .await
            .map_err(|err| internal_error("trace", err));

        if let Some(proxy) = &self.state.tx_sender.0.proxy {
            // We're running an external node - check the proxy cache in
            // case the transaction was proxied but not yet synced back to us
            if let Ok(Some(_tx)) = &transaction {
            } else {
                if let Some(tx) = proxy.find_tx(tx_hash).await {
                    transaction = Ok(Some(tx.into()));
                }
                if !matches!(transaction, Ok(Some(_))) {
                    // If the transaction is not in the db or cache, query main node
                    transaction = proxy
                        .request_tx(tx_hash.into())
                        .await
                        .map_err(|err| internal_error("trace_transaction", err));
                }
            }
        }

        if !matches!(transaction, Ok(Some(_))) {
            return vec![];
        }
        let transaction = transaction.unwrap().unwrap();

        let call_trace = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .transactions_dal()
            .get_call_trace(tx_hash)
            .await;
        if call_trace.is_none() {
            return vec![];
        }

        let call_trace = call_trace.unwrap();

        flat_call(
            call_trace,
            transaction.transaction_index.unwrap().as_usize(),
            tx_hash,
            transaction.block_number.unwrap().as_u64(),
            transaction.block_hash.unwrap(),
            &mut Vec::new(),
        )
    }
}
