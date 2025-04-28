use std::{fmt::Debug, sync::Arc};

use anyhow::Context;
use zksync_interop_switch::{
    db::InMemoryDb, DbClient, DestinationChain, InteropSwitch, SourceChain,
};
use zksync_types::url::SensitiveUrl;
use zksync_web3_decl::client::{Client, L2};

use crate::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for sequencer L1 gas interfaces.
/// Adds several resources that depend on L1 gas price.
#[derive(Debug)]
pub struct InteropSwitchLayer {
    src_chains: Vec<SensitiveUrl>,
    dst_chains: Vec<Box<dyn DestinationChain>>,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub interop_switch_task: InteropSwitch<InMemoryDb>,
}

impl InteropSwitchLayer {
    pub fn new(src_chains: Vec<SensitiveUrl>, dst_chains: Vec<Box<dyn DestinationChain>>) -> Self {
        Self {
            src_chains,
            dst_chains,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for InteropSwitchLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "interop_switch"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let clients = self
            .src_chains
            .iter()
            .map(|url| {
                Client::http(url.clone())
                    .context("Client::new()")
                    .unwrap()
                    .build()
            })
            .collect::<Vec<Client<L2>>>();

        let mut src_chains = vec![];
        for client in &clients {
            src_chains.push(SourceChain::new(client.clone()).await);
        }

        Ok(Output {
            interop_switch_task: InteropSwitch::new(
                src_chains,
                self.dst_chains,
                InMemoryDb::default(),
            ),
        })
    }
}

#[async_trait::async_trait]
impl Task for InteropSwitch<InMemoryDb> {
    fn id(&self) -> TaskId {
        "interop_switch".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
