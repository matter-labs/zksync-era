use std::time::Duration;

use anyhow::Context;
use zksync_basic_types::web3::contract::{Detokenize, Tokenize};
use zksync_concurrency::{ctx::Ctx, error::Wrap as _};
use zksync_contracts::consensus_l2_contracts;
use zksync_consensus_roles::attester;
use zksync_node_api_server::{
    execution_sandbox::{BlockArgs, BlockStartInfo},
    tx_sender::TxSender,
};
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    api,
    ethabi,
    fee::Fee,
    l2::L2Tx,
    transaction_request::CallOverrides,
    Nonce, U256,
};

use crate::storage::{
    registry_contract::abi,
    ConnectionPool,
};

/// A struct for reading data from consensus L2 contracts.
#[derive(Debug)]
pub struct VMReader {
    pool: ConnectionPool,
    tx_sender: TxSender,
    registry_contract: ethabi::Contract,
    registry_address: ethabi::Address,
    // contract functions:
    attester_committee_size: ethabi::Function,
    attester_committee: ethabi::Function,
}

#[allow(dead_code)]
impl VMReader {
    /// Constructs a new `VMReader` instance.
    pub fn new(pool: ConnectionPool, tx_sender: TxSender, registry_address: ethabi::Address) -> Self {
        let c = consensus_l2_contracts::load_consensus_registry_contract();
        Self {
            pool,
            tx_sender,
            attester_committee_size: c.function("attesterCommitteeSize").context("attesterCommitteeSize")?.clone(),
            attester_committee : c.function("attesterCommittee").context("attesterCommittee")?.clone(),
            registry_contract: c,
            registry_address,
        }
    }

    /// Reads attester committee from the registry contract.
    /// It's implemented by dispatching multiple read transactions (a.k.a. `eth_call` requests),
    /// each one carries an instantiation of a separate VM execution sandbox.
    // TODO(moshababo|BFT-493): optimize this process to reuse a single execution sandbox
    // across one or multiple read sessions.
    pub async fn read_attester_committee(
        &self,
        ctx: &Ctx,
        // wtf is this?
        block_id: api::BlockId,
    ) -> anyhow::Result<attester::Committee> {
        let block_args = self
            .block_args(ctx, block_id)
            .await
            .context("block_args()")?;
        self
            .read_attester_committee(block_args)
            .await
            .context("read_attester_committee()")
    }

    pub async fn block_args(&self, ctx: &Ctx, block_id: api::BlockId) -> anyhow::Result<BlockArgs> {
        let mut conn = self.pool.connection(ctx).await.wrap("connection()")?.0;
        let start_info = BlockStartInfo::new(&mut conn, /*max_cache_age=*/ Duration::from_secs(10))
            .await
            .unwrap();

        BlockArgs::new(&mut conn, block_id, &start_info)
            .await
            .context("BlockArgs::new")
    }

    pub async fn contract_deployed(&self, args: BlockArgs) -> bool {
        self.read_attester_committee_size(args).await.is_ok()
    }

    pub async fn read_attester_committee(
        &self,
        args: BlockArgs,
    ) -> anyhow::Result<attester::Committee> {
        let n = self.read_attester_committee_size(args).await.context("read_attester_committee_size()")?;
        let mut committee = vec![];
        for i in 0..n {
            committee.push(self.read_attester(args, i).await.with_context(||format!("read_attester({i})"))?);
        }
        attester::Committee::new(committee.into_iter()).context("attester::Committee::new()")
    }

    async fn read_attester_committee_size(&self, args: BlockArgs) -> anyhow::Result<usize> {
        self.call::<U256>(args, &self.attester_committee_size, ())?.try_into().context("overflow")
    }

    async fn read_attester(&self, args: BlockArgs, i: usize) -> anyhow::Result<attester::WeightedAttester> {
        self.call::<abi::Attester>(args, &self.attester_committee, U256::from(i))?.parse()
    }

    async fn call<Output: Detokenize>(&self, args: BlockArgs, func: &ethabi::Function, input: impl Tokenize) -> anyhow::Result<Output> {
        let tx = L2Tx::new(
            self.registry_address,
            func.encode_input(&input.into_tokens()),
            Nonce(0),
            Fee {
                gas_limit: U256::from(2000000000u32),
                max_fee_per_gas: U256::zero(),
                max_priority_fee_per_gas: U256::zero(),
                gas_per_pubdata_limit: U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE),
            },
            ethabi::Address::zero(),
            U256::zero(),
            vec![],
            Default::default(),
        );
        let overrides = CallOverrides { enforced_base_fee: None };
        let output = self.tx_sender.eth_call(args, overrides, tx, None).await.context("tx_sender.eth_call()");
        Output::from_tokens(func.decode_output(&output).context("decode_output()")?).context("Output::from_tokens()")
    }
}
