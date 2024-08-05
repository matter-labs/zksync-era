//! Storage test helpers.

use anyhow::Context as _;
use zksync_concurrency::{ctx, ctx::Ctx, error::Wrap as _, time};
use zksync_consensus_roles::validator;
use zksync_contracts::{consensus_l2_contracts, BaseSystemContracts, TestContract};
use zksync_dal::CoreDal as _;
use zksync_node_genesis::{insert_genesis_batch, mock_genesis_config, GenesisParams};
use zksync_node_test_utils::{recover, snapshot, Snapshot};
use zksync_state_keeper::testonly::fee;
use zksync_test_account::{Account, DeployContractsTx, TxType};
use zksync_types::{
    commitment::L1BatchWithMetadata,
    ethabi::{Address, Contract, Token},
    protocol_version::ProtocolSemanticVersion,
    system_contracts::get_system_smart_contracts,
    Execute, L1BatchNumber, L2BlockNumber, ProtocolVersionId, Transaction,
};

use super::ConnectionPool;
use crate::testonly::StateKeeper;

pub(crate) fn mock_genesis_params(protocol_version: ProtocolVersionId) -> GenesisParams {
    let mut cfg = mock_genesis_config();
    cfg.protocol_version = Some(ProtocolSemanticVersion {
        minor: protocol_version,
        patch: 0.into(),
    });
    GenesisParams::from_genesis_config(
        cfg,
        BaseSystemContracts::load_from_disk(),
        get_system_smart_contracts(),
    )
    .unwrap()
}

impl ConnectionPool {
    pub(crate) async fn test(
        from_snapshot: bool,
        protocol_version: ProtocolVersionId,
    ) -> ConnectionPool {
        match from_snapshot {
            true => {
                ConnectionPool::from_snapshot(Snapshot::new(
                    L1BatchNumber(23),
                    L2BlockNumber(87),
                    vec![],
                    mock_genesis_params(protocol_version),
                ))
                .await
            }
            false => ConnectionPool::from_genesis(protocol_version).await,
        }
    }

    /// Waits for the `number` L2 block to have a certificate.
    pub async fn wait_for_block_certificate(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
        while self
            .connection(ctx)
            .await
            .wrap("connection()")?
            .block_certificate(ctx, number)
            .await
            .wrap("block_certificate()")?
            .is_none()
        {
            ctx.sleep(POLL_INTERVAL).await?;
        }
        Ok(())
    }

    /// Waits for the `number` L1 batch.
    pub async fn wait_for_batch(
        &self,
        ctx: &ctx::Ctx,
        number: L1BatchNumber,
    ) -> ctx::Result<L1BatchWithMetadata> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        loop {
            if let Some(payload) = self
                .connection(ctx)
                .await
                .wrap("connection()")?
                .batch(ctx, number)
                .await
                .wrap("batch()")?
            {
                return Ok(payload);
            }
            ctx.sleep(POLL_INTERVAL).await?;
        }
    }

    /// Takes a storage snapshot at the last sealed L1 batch.
    pub(crate) async fn snapshot(&self, ctx: &ctx::Ctx) -> ctx::Result<Snapshot> {
        let mut conn = self.connection(ctx).await.wrap("connection()")?;
        Ok(ctx.wait(snapshot(&mut conn.0)).await?)
    }

    /// Constructs a new db initialized with genesis state.
    pub(crate) async fn from_genesis(protocol_version: ProtocolVersionId) -> Self {
        let pool = zksync_dal::ConnectionPool::test_pool().await;
        {
            let mut storage = pool.connection().await.unwrap();
            insert_genesis_batch(&mut storage, &mock_genesis_params(protocol_version))
                .await
                .unwrap();
        }
        Self(pool)
    }

    /// Recovers storage from a snapshot.
    pub(crate) async fn from_snapshot(snapshot: Snapshot) -> Self {
        let pool = zksync_dal::ConnectionPool::test_pool().await;
        {
            let mut storage = pool.connection().await.unwrap();
            recover(&mut storage, snapshot).await;
        }
        Self(pool)
    }

    /// Waits for `want_last` block to have certificate then fetches all L2 blocks with certificates.
    pub async fn wait_for_block_certificates(
        &self,
        ctx: &ctx::Ctx,
        want_last: validator::BlockNumber,
    ) -> ctx::Result<Vec<validator::FinalBlock>> {
        self.wait_for_block_certificate(ctx, want_last).await?;
        let mut conn = self.connection(ctx).await.wrap("connection()")?;
        let range = conn
            .block_certificates_range(ctx)
            .await
            .wrap("certificates_range()")?;
        assert_eq!(want_last.next(), range.next());
        let mut blocks: Vec<validator::FinalBlock> = vec![];
        for i in range.first.0..range.next().0 {
            let i = validator::BlockNumber(i);
            let block = conn.block(ctx, i).await.context("block()")?.unwrap();
            blocks.push(block);
        }
        Ok(blocks)
    }

    /// Same as `wait_for_certificates`, but additionally verifies all the blocks against genesis.
    pub async fn wait_for_block_certificates_and_verify(
        &self,
        ctx: &ctx::Ctx,
        want_last: validator::BlockNumber,
    ) -> ctx::Result<Vec<validator::FinalBlock>> {
        let blocks = self.wait_for_block_certificates(ctx, want_last).await?;
        let genesis = self
            .connection(ctx)
            .await
            .wrap("connection()")?
            .genesis(ctx)
            .await
            .wrap("genesis()")?
            .context("genesis is missing")?;
        for block in &blocks {
            block.verify(&genesis).context(block.number())?;
        }
        Ok(blocks)
    }

    pub async fn prune_batches(
        &self,
        ctx: &ctx::Ctx,
        last_batch: L1BatchNumber,
    ) -> ctx::Result<()> {
        let mut conn = self.connection(ctx).await.context("connection()")?;
        let (_, last_block) = ctx
            .wait(
                conn.0
                    .blocks_dal()
                    .get_l2_block_range_of_l1_batch(last_batch),
            )
            .await?
            .context("get_l2_block_range_of_l1_batch()")?
            .context("batch not found")?;
        conn.0
            .pruning_dal()
            .soft_prune_batches_range(last_batch, last_block)
            .await
            .context("soft_prune_batches_range()")?;
        conn.0
            .pruning_dal()
            .hard_prune_batches_range(last_batch, last_block)
            .await
            .context("hard_prune_batches_range()")?;
        Ok(())
    }
}

/// A struct for writing to consensus L2 contracts.
#[derive(Debug)]
pub struct VMWriter {
    pool: ConnectionPool,
    node: StateKeeper,
    account: Account,
    registry_contract: TestContract,
    pub deploy_tx: DeployContractsTx,
}

impl VMWriter {
    /// Constructs a new `VMWriter` instance.
    pub fn new(
        pool: ConnectionPool,
        node: StateKeeper,
        mut account: Account,
        owner: Address,
    ) -> Self {
        let registry_contract = consensus_l2_contracts::load_consensus_registry_contract_in_test();
        let deploy_tx = account.get_deploy_tx_with_factory_deps(
            &registry_contract.bytecode,
            Some(&[Token::Address(owner)]),
            vec![],
            TxType::L2,
        );

        Self {
            pool,
            node,
            account,
            registry_contract,
            deploy_tx,
        }
    }

    /// Deploys the consensus registry contract and adds nodes to it.
    pub async fn deploy(&mut self) {
        let mut txs: Vec<Transaction> = vec![];
        txs.push(self.deploy_tx.tx.clone());
        self.node.push_block(&txs).await
    }

    pub async fn add_nodes(&mut self, nodes: &[&[Token]]) {
        let mut txs: Vec<Transaction> = vec![];
        for node in nodes {
            let tx = self.gen_tx_add(node);
            txs.push(tx);
        }
        self.node.push_block(&txs).await
    }

    pub async fn set_committees(&mut self) {
        let mut txs: Vec<Transaction> = vec![];
        txs.push(self.gen_tx_set_validator_committee());
        txs.push(self.gen_tx_set_attester_committee());
        self.node.push_block(&txs).await
    }

    pub async fn remove_nodes(&mut self, nodes: &[&[Token]]) {
        let mut txs: Vec<Transaction> = vec![];
        for node in nodes {
            let tx = self.gen_tx_remove(&vec![node[0].clone()]);
            txs.push(tx);
        }
        self.node.push_block(&txs).await
    }

    pub async fn seal_batch_and_wait(&mut self, ctx: &Ctx) -> L1BatchNumber {
        self.node.seal_batch().await;
        self.pool
            .wait_for_payload(ctx, self.node.last_block())
            .await
            .unwrap()
            .l1_batch_number
    }

    fn gen_tx_add(&mut self, input: &[Token]) -> Transaction {
        let calldata = self
            .registry_contract
            .contract
            .function("add")
            .unwrap()
            .encode_input(input)
            .unwrap();
        self.gen_tx(self.deploy_tx.address, calldata)
    }

    fn gen_tx_remove(&mut self, input: &[Token]) -> Transaction {
        let calldata = self
            .registry_contract
            .contract
            .function("remove")
            .unwrap()
            .encode_input(input)
            .unwrap();
        self.gen_tx(self.deploy_tx.address, calldata)
    }

    fn gen_tx_set_validator_committee(&mut self) -> Transaction {
        let calldata = self
            .registry_contract
            .contract
            .function("setValidatorCommittee")
            .unwrap()
            .short_signature()
            .to_vec();
        self.gen_tx(self.deploy_tx.address, calldata)
    }

    fn gen_tx_set_attester_committee(&mut self) -> Transaction {
        let calldata = self
            .registry_contract
            .contract
            .function("setAttesterCommittee")
            .unwrap()
            .short_signature()
            .to_vec();
        self.gen_tx(self.deploy_tx.address, calldata)
    }

    fn gen_tx(&mut self, contract_address: Address, calldata: Vec<u8>) -> Transaction {
        self.account.get_l2_tx_for_execute(
            Execute {
                contract_address,
                calldata,
                value: Default::default(),
                factory_deps: vec![],
            },
            Some(fee(10_000_000)),
        )
    }
}
