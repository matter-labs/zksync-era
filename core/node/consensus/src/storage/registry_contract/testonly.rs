use zksync_basic_types::{ethabi, L1BatchNumber};
use zksync_concurrency::ctx::Ctx;
use zksync_contracts::{consensus_l2_contracts as contracts};
use zksync_state_keeper::testonly::fee;
use zksync_test_account::{Account, DeployContractsTx, TxType};
use zksync_types::{Execute, Transaction};
use crate::{storage::ConnectionPool, testonly::StateKeeper};

/// A struct for writing to consensus L2 contracts.
#[derive(Debug)]
pub struct VMWriter {
    pool: ConnectionPool,
    node: StateKeeper,
    account: Account,
    contract: contracts::ConsensusRegistry,
    pub deploy_tx: DeployContractsTx,
}

impl VMWriter {
    /// Constructs a new `VMWriter` instance.
    pub fn new(
        pool: ConnectionPool,
        node: StateKeeper,
        mut account: Account,
        owner: ethabi::Address,
    ) -> Self {
        let contract = contracts::ConsensusRegistry::bytecode();
        let deploy_tx = account.get_deploy_tx_with_factory_deps(
            &contract.bytecode,
            Some(&owner.into_tokens()),
            vec![],
            TxType::L2,
        );

        Self {
            pool,
            node,
            account,
            contract: contracts::ConsensusRegistry::load(),
            deploy_tx,
        }
    }

    /// Deploys the consensus registry contract and adds nodes to it.
    pub async fn deploy(&mut self) {
        let mut txs: Vec<Transaction> = vec![];
        txs.push(self.deploy_tx.tx.clone());
        self.node.push_block(&txs).await
    }

    pub async fn add_nodes(&mut self, nodes: &[&[ethabi::Token]]) {
        let txs: Vec<_> = nodes.iter().map(|n|self.gen_tx_add(n)).collect();
        self.node.push_block(&txs).await
    }

    pub async fn set_committees(&mut self) {
        self.node.push_block(&[
            self.gen_tx_set_validator_committee(),
            self.gen_tx_set_attester_committee(),
        ]).await
    }

    pub async fn remove_nodes(&mut self, nodes: &[&[ethabi::Token]]) {
        let txs: Vec<_> = nodes.iter().map(|n|self.gen_tx_remove(n)).collect();
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

    fn gen_tx_add(&mut self, input: &[ethabi::Token]) -> Transaction {
        let calldata = self
            .registry_contract
            .contract
            .function("add")
            .unwrap()
            .encode_input(input)
            .unwrap();
        self.gen_tx(self.deploy_tx.address, calldata)
    }

    fn gen_tx_remove(&mut self, input: &[ethabi::Token]) -> Transaction {
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
        let input = self
            .registry_contract
            .contract
            .function("setAttesterCommittee")
            .unwrap()
            .encode_input(&[])
            .unwrap();
        self.gen_tx(self.deploy_tx.address, input)
    }

    fn gen_tx(&mut self, contract_address: ethabi::Address, calldata: Vec<u8>) -> Transaction {
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
