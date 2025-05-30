use std::sync::Arc;

use zksync_dal::node::{MasterPool, PoolResource};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

use crate::tx_sender::{master_pool_sink::MasterPoolSink, tx_sink::TxSink};

/// Wiring layer for [`MasterPoolSink`], [`TxSink`](zksync_node_api_server::tx_sender::tx_sink::TxSink) implementation.
pub struct MasterPoolSinkLayer;

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    tx_sink: Arc<dyn TxSink>,
}

#[async_trait::async_trait]
impl WiringLayer for MasterPoolSinkLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "master_pool_sink_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        Ok(Output {
            tx_sink: Arc::new(MasterPoolSink::new(pool)),
        })
    }
}
