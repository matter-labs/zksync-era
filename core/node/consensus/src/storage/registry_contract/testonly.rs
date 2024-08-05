use zksync_basic_types::{
    ethabi::{Address, Token},
    L1BatchNumber,
};
use zksync_concurrency::ctx::Ctx;
use zksync_contracts::{consensus_l2_contracts, TestContract};
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
