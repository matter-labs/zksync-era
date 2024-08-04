//! Storage implementation based on DAL.

use zksync_basic_types::web3::contract::{Tokenizable, Tokenize};
use zksync_concurrency::ctx;
use zksync_consensus_crypto::ByteFmt as _;
use zksync_consensus_roles::{attester, validator};
use zksync_dal::consensus_dal;
use zksync_node_sync::{
    fetcher::{FetchedBlock, IoCursorExt as _},
    sync_action::ActionQueueSender,
    SyncState,
};
use zksync_state_keeper::io::common::IoCursor;

mod connection;
mod store;

pub(crate) use connection::*;
pub(crate) use store::*;
use zksync_basic_types::{
    ethabi::{Address, Bytes, Token},
    web3::contract::{Detokenize, Error},
    H160, U256,
};

pub(crate) mod denormalizer;
#[cfg(test)]
pub(crate) mod testonly;
#[cfg(test)]
mod tests;
mod vm_reader;

#[derive(thiserror::Error, Debug)]
pub enum InsertCertificateError {
    #[error(transparent)]
    Canceled(#[from] ctx::Canceled),
    #[error(transparent)]
    Inner(#[from] consensus_dal::InsertCertificateError),
}

impl From<ctx::Error> for InsertCertificateError {
    fn from(err: ctx::Error) -> Self {
        match err {
            ctx::Error::Canceled(err) => Self::Canceled(err),
            ctx::Error::Internal(err) => Self::Inner(err.into()),
        }
    }
}

#[derive(Debug)]
pub(crate) struct PayloadQueue {
    inner: IoCursor,
    actions: ActionQueueSender,
    sync_state: SyncState,
}

impl PayloadQueue {
    pub(crate) fn next(&self) -> validator::BlockNumber {
        validator::BlockNumber(self.inner.next_l2_block.0.into())
    }

    /// Advances the cursor by converting the block into actions and pushing them
    /// to the actions queue.
    /// Does nothing and returns `Ok(())` if the block has been already processed.
    /// Returns an error if a block with an earlier block number was expected.
    pub(crate) async fn send(&mut self, block: FetchedBlock) -> anyhow::Result<()> {
        let want = self.inner.next_l2_block;
        // Some blocks are missing.
        if block.number > want {
            anyhow::bail!("expected {want:?}, got {:?}", block.number);
        }
        // Block already processed.
        if block.number < want {
            return Ok(());
        }
        self.actions.push_actions(self.inner.advance(block)).await?;
        Ok(())
    }
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct CommitteeValidator {
    pub node_owner: Address,
    pub weight: usize,
    pub pub_key: Vec<u8>,
    pub pop: Vec<u8>,
}

impl TryInto<consensus_dal::Validator> for CommitteeValidator {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<consensus_dal::Validator, Self::Error> {
        Ok(consensus_dal::Validator {
            pub_key: validator::PublicKey::decode(&self.pub_key).unwrap(),
            weight: self.weight as u64,
            proof_of_possession: validator::Signature::decode(&self.pop).unwrap(),
        })
    }
}

impl Detokenize for CommitteeValidator {
    fn from_tokens(tokens: Vec<Token>) -> Result<Self, Error> {
        Ok(Self {
            node_owner: H160::from_token(
                tokens
                    .get(0)
                    .ok_or_else(|| Error::Other("tokens[0] missing".to_string()))?
                    .clone(),
            )?,
            weight: U256::from_token(
                tokens
                    .get(1)
                    .ok_or_else(|| Error::Other("tokens[1] missing".to_string()))?
                    .clone(),
            )?
            .as_usize(),
            pub_key: Bytes::from_token(
                tokens
                    .get(2)
                    .ok_or_else(|| Error::Other("tokens[2] missing".to_string()))?
                    .clone(),
            )?,
            pop: Bytes::from_token(
                tokens
                    .get(3)
                    .ok_or_else(|| Error::Other("tokens[3] missing".to_string()))?
                    .clone(),
            )?,
        })
    }
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct CommitteeAttester {
    pub weight: usize,
    pub node_owner: Address,
    pub pub_key: Vec<u8>,
}

impl TryInto<consensus_dal::Attester> for CommitteeAttester {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<consensus_dal::Attester, Self::Error> {
        Ok(consensus_dal::Attester {
            pub_key: attester::PublicKey::decode(&self.pub_key).unwrap(),
            weight: self.weight as u64,
        })
    }
}

impl Detokenize for CommitteeAttester {
    fn from_tokens(tokens: Vec<Token>) -> Result<Self, Error> {
        Ok(Self {
            weight: U256::from_token(
                tokens
                    .get(0)
                    .ok_or_else(|| Error::Other("tokens[0] missing".to_string()))?
                    .clone(),
            )?
            .as_usize(),
            node_owner: H160::from_token(
                tokens
                    .get(1)
                    .ok_or_else(|| Error::Other("tokens[1] missing".to_string()))?
                    .clone(),
            )?,
            pub_key: Bytes::from_token(
                tokens
                    .get(2)
                    .ok_or_else(|| Error::Other("tokens[2] missing".to_string()))?
                    .clone(),
            )?,
        })
    }
}
