use async_trait::async_trait;

use crate::l1_fetcher::types::CommitBlock;

pub mod genesis;
mod main_node;
pub mod tree;

pub mod snapshot;

#[async_trait]
pub trait Processor {
    async fn process_blocks(&mut self, blocks: Vec<CommitBlock>);
}
