use std::time::Duration;

use zksync_concurrency::ctx::Ctx;
use zksync_contracts::load_contract;
use zksync_node_api_server::{execution_sandbox::BlockStartInfo, tx_sender::TxSender};
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    api::BlockId,
    ethabi::{Address, Contract, Function, Token},
    fee::Fee,
    l2::L2Tx,
    transaction_request::CallOverrides,
    Nonce, U256,
};

use crate::storage::ConnectionPool;

pub struct VMReader {
    pool: ConnectionPool,
    tx_sender: TxSender,
    registry_contract: Contract,
    registry_address: Address,
}

impl VMReader {
    pub fn new(pool: ConnectionPool, tx_sender: TxSender, registry_address: Address) -> Self {
        let registry_contract = load_contract("contracts/l2-contracts/artifacts-zk/contracts/ConsensusRegistry.sol/ConsensusRegistry.json");
        Self {
            pool,
            tx_sender,
            registry_contract,
            registry_address,
        }
    }

    pub async fn read_validator_committee(
        &mut self,
        ctx: &Ctx,
        block_id: BlockId,
    ) -> Vec<CommitteeValidator> {
        let mut committee = vec![];
        let validator_committee_size = self.read_validator_committee_size(ctx, block_id).await;
        for i in 0..validator_committee_size {
            let committee_validator = self.read_committee_validator(ctx, block_id, i).await;
            committee.push(committee_validator)
        }
        committee
    }

    pub async fn read_attester_committee(
        &mut self,
        ctx: &Ctx,
        block_id: BlockId,
    ) -> Vec<CommitteeAttester> {
        let mut committee = vec![];
        let attester_committee_size = self.read_attester_committee_size(ctx, block_id).await;
        for i in 0..attester_committee_size {
            let committee_validator = self.read_committee_attester(ctx, block_id, i).await;
            committee.push(committee_validator)
        }
        committee
    }

    async fn read_validator_committee_size(&mut self, ctx: &Ctx, block_id: BlockId) -> usize {
        let func = self
            .registry_contract
            .function("validatorCommitteeSize")
            .unwrap()
            .clone();

        let tx = self.gen_l2_call_tx(self.registry_address, func.short_signature().to_vec());

        let res = self.eth_call(ctx, block_id, tx).await;

        func.decode_output(&res).unwrap()[0]
            .clone()
            .into_uint()
            .unwrap()
            .as_usize()
    }

    async fn read_attester_committee_size(&mut self, ctx: &Ctx, block_id: BlockId) -> usize {
        let func = self
            .registry_contract
            .function("attesterCommitteeSize")
            .unwrap()
            .clone();
        let tx = self.gen_l2_call_tx(self.registry_address, func.short_signature().to_vec());

        let res = self.eth_call(ctx, block_id, tx).await;
        func.decode_output(&res).unwrap()[0]
            .clone()
            .into_uint()
            .unwrap()
            .as_usize()
    }

    async fn read_attester(
        &mut self,
        ctx: &Ctx,
        block_id: BlockId,
        node_owner: Address,
    ) -> (usize, Vec<u8>, bool) {
        let func = self
            .registry_contract
            .function("attesters")
            .unwrap()
            .clone();
        let tx = self.gen_l2_call_tx(
            self.registry_address,
            func.encode_input(&[Token::Address(node_owner)]).unwrap(),
        );
        let res = self.eth_call(ctx, block_id, tx).await;
        let tokens = func.decode_output(&res).unwrap();
        (
            tokens[0].clone().into_uint().unwrap().as_usize(),
            tokens[1].clone().into_bytes().unwrap(),
            tokens[2].clone().into_bool().unwrap(),
        )
    }

    async fn read_committee_validator(
        &mut self,
        ctx: &Ctx,
        block_id: BlockId,
        idx: usize,
    ) -> CommitteeValidator {
        let func = self
            .registry_contract
            .function("validatorCommittee")
            .unwrap()
            .clone();
        let tx = self.gen_l2_call_tx(
            self.registry_address,
            func.encode_input(&[Token::Uint(zksync_types::U256::from(idx))])
                .unwrap(),
        );

        let res = self.eth_call(ctx, block_id, tx).await;
        let tokens = func.decode_output(&res).unwrap();
        CommitteeValidator {
            node_owner: tokens[0].clone().into_address().unwrap(),
            weight: tokens[1].clone().into_uint().unwrap().as_usize(),
            pub_key: tokens[2].clone().into_bytes().unwrap(),
            pop: tokens[3].clone().into_bytes().unwrap(),
        }
    }

    async fn read_committee_attester(
        &mut self,
        ctx: &Ctx,
        block_id: BlockId,
        idx: usize,
    ) -> CommitteeAttester {
        let func = self
            .registry_contract
            .function("attesterCommittee")
            .unwrap()
            .clone();
        let tx = self.gen_l2_call_tx(
            self.registry_address,
            func.encode_input(&[Token::Uint(zksync_types::U256::from(idx))])
                .unwrap(),
        );

        let res = self.eth_call(ctx, block_id, tx).await;
        let tokens = func.decode_output(&res).unwrap();
        CommitteeAttester {
            weight: tokens[0].clone().into_uint().unwrap().as_usize(),
            node_owner: tokens[1].clone().into_address().unwrap(),
            pub_key: tokens[2].clone().into_bytes().unwrap(),
        }
    }

    async fn read_address(
        &mut self,
        ctx: &Ctx,
        block_id: BlockId,
        contract_address: Address,
        func: Function,
    ) -> Address {
        let tx = self.gen_l2_call_tx(contract_address, func.encode_input(&vec![]).unwrap());

        let res = self.eth_call(ctx, block_id, tx).await;
        let tokens = func.decode_output(&res).unwrap();
        tokens[0].clone().into_address().unwrap()
    }

    async fn eth_call(&mut self, ctx: &Ctx, block_id: BlockId, tx: L2Tx) -> Vec<u8> {
        let mut conn = self.pool.connection(ctx).await.unwrap().0;
        let start_info = BlockStartInfo::new(&mut conn, Duration::from_secs(10))
            .await
            .unwrap();
        let block_args = zksync_node_api_server::execution_sandbox::BlockArgs::new(
            &mut conn,
            block_id,
            &start_info,
        )
        .await
        .unwrap();
        let call_overrides = CallOverrides {
            enforced_base_fee: None,
        };

        let res = self
            .tx_sender
            .eth_call(block_args, call_overrides, tx)
            .await
            .unwrap();

        res
    }

    fn gen_l2_call_tx(&mut self, contract_address: Address, calldata: Vec<u8>) -> L2Tx {
        L2Tx::new(
            contract_address,
            calldata,
            Nonce(0),
            Fee {
                gas_limit: U256::from(2000000000u32),
                max_fee_per_gas: U256::zero(),
                max_priority_fee_per_gas: U256::zero(),
                gas_per_pubdata_limit: U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE),
            },
            Address::zero(),
            U256::zero(),
            vec![],
            Default::default(),
        )
    }
}

#[derive(Debug, Default)]
pub struct CommitteeValidator {
    pub node_owner: Address,
    pub weight: usize,
    pub pub_key: Vec<u8>,
    pub pop: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct CommitteeAttester {
    pub weight: usize,
    pub node_owner: Address,
    pub pub_key: Vec<u8>,
}
