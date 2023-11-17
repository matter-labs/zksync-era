use anyhow::Context as _;
use zksync_concurrency::{ctx, time};
use zksync_consensus_roles::validator;
use zksync_types::block::ConsensusBlockFields;
use zksync_types::{Address, MiniblockNumber};

mod payload;
mod proto;

pub(crate) use payload::Payload;
