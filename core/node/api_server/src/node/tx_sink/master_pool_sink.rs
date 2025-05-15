use zksync_dal::node::{MasterPool, PoolResource};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

use crate::{node::TxSinkResource, tx_sender::master_pool_sink::MasterPoolSink};

/// Wiring layer for [`MasterPoolSink`], [`TxSink`](zksync_node_api_server::tx_sender::tx_sink::TxSink) implementation.
pub struct MasterPoolSinkLayer;

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    pub tx_sink: TxSinkResource,
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
            tx_sink: MasterPoolSink::new(pool).into(),
        })
    }
}
