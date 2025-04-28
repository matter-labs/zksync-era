use std::{fmt::Debug, marker::PhantomData, sync::Arc};

use anyhow::bail;
use tokio::sync::watch::Receiver;

use crate::types::{InteropBundle, InteropTrigger};
pub use crate::{
    chain::{DestinationChain, LocalDestinationChain, SourceChain},
    db::{in_memory::InMemoryDb, DbClient},
    listener::InteropListener,
};

mod chain;
pub mod db;
mod listener;
mod types;

#[derive(Debug)]
pub struct InteropSwitch<C: DbClient + Debug> {
    src_chains: Vec<SourceChain>,
    dst_chains: Vec<Arc<dyn DestinationChain>>,
    db: C,
}

pub struct InteropSender<C: DbClient> {
    dst_chain: Arc<dyn DestinationChain>,
    _phantom_data: PhantomData<C>,
}

impl<C: DbClient> InteropSender<C> {
    pub async fn start(self, db: C) -> anyhow::Result<()> {
        // Start sending interop transactions to the destination chain
        // and handle them accordingly.
        Ok(())
    }
}

impl<C: DbClient + Debug + Send + Sync + 'static> InteropSwitch<C> {
    pub fn new(
        src_chains: Vec<SourceChain>,
        dst_chains: Vec<Box<dyn DestinationChain>>,
        db: C,
    ) -> Self {
        let dst_chains: Vec<Arc<dyn DestinationChain>> =
            dst_chains.into_iter().map(Into::into).collect();
        Self {
            src_chains,
            dst_chains,
            db,
        }
    }

    pub async fn run(mut self, mut stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        println!("Starting interop switch...");
        let mut tasks = vec![];
        for src_chain in &self.src_chains {
            for dst_chain in &self.dst_chains {
                let listener = InteropListener::new(src_chain.clone(), dst_chain.chain_id(), 10000);
                tasks.push(tokio::spawn(listener.start(self.db.clone())));
            }
        }

        for dst_chain in &self.dst_chains {
            let sender = InteropSender {
                dst_chain: dst_chain.clone(),
                _phantom_data: Default::default(),
            };
            tasks.push(tokio::spawn(sender.start(self.db.clone())));
        }

        let job_completion = futures::future::try_join_all(tasks);

        tokio::select! {
            res = job_completion => {
                res?;
            },
            _ = stop_receiver.changed() => {
                bail!("Received stop signal");
            }
        }
        Ok(())
    }
}
