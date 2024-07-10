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
    consensus_authority_contract: Contract,
    validator_registry_address: Option<Address>,
    validator_registry_contract: Contract,
    attester_registry_address: Option<Address>,
    attester_registry_contract: Contract,
}

impl VMReader {
    pub async fn new(
        ctx: &Ctx,
        block_id: BlockId,
        pool: ConnectionPool,
        tx_sender: TxSender,
        consensus_authority_address: Address,
    ) -> Self {
        let consensus_authority_contract = load_contract("contracts/l2-contracts/artifacts-zk/contracts/ConsensusAuthority.sol/ConsensusAuthority.json");
        let validator_registry_contract = load_contract("contracts/l2-contracts/artifacts-zk/contracts/ValidatorRegistry.sol/ValidatorRegistry.json");
        let attester_registry_contract = load_contract("contracts/l2-contracts/artifacts-zk/contracts/AttesterRegistry.sol/AttesterRegistry.json");

        let mut reader = Self {
            pool,
            tx_sender,
            consensus_authority_contract,
            validator_registry_address: None,
            validator_registry_contract,
            attester_registry_address: None,
            attester_registry_contract,
        };

        reader.validator_registry_address = Some(
            reader
                .read_address(
                    ctx,
                    block_id,
                    consensus_authority_address,
                    reader
                        .consensus_authority_contract
                        .function("validatorRegistry")
                        .unwrap()
                        .clone(),
                )
                .await,
        );

        reader.attester_registry_address = Some(
            reader
                .read_address(
                    ctx,
                    block_id,
                    consensus_authority_address,
                    reader
                        .consensus_authority_contract
                        .function("attesterRegistry")
                        .unwrap()
                        .clone(),
                )
                .await,
        );

        reader
    }

    pub async fn read_validator_committee(
        &mut self,
        ctx: &Ctx,
        block_id: BlockId,
    ) -> Vec<CommitteeValidator> {
        let mut validators = vec![];
        let num_committee_validators = self.read_num_committee_validators(ctx, block_id).await;
        for i in 0..num_committee_validators {
            let committee_validator = self.read_committee_validator(ctx, block_id, i).await;
            let validator = self
                .read_validator(ctx, block_id, committee_validator.0)
                .await;

            validators.push(CommitteeValidator {
                node_owner: committee_validator.0,
                weight: committee_validator.1,
                pub_key: committee_validator.2,
                pop: validator.2,
            })
        }

        validators
    }

    pub async fn read_attester_committee(
        &mut self,
        ctx: &Ctx,
        block_id: BlockId,
    ) -> Vec<CommitteeAttester> {
        let mut attesters = vec![];
        let num_committee_attesters = self.read_num_committee_attesters(ctx, block_id).await;
        for i in 0..num_committee_attesters {
            let committee_attester = self.read_committee_attester(ctx, block_id, i).await;
            let attester = self
                .read_attester(ctx, block_id, committee_attester.1)
                .await;

            attesters.push(CommitteeAttester {
                node_owner: committee_attester.1,
                weight: committee_attester.0,
                pub_key: attester.1,
            })
        }
        attesters
    }

    async fn read_num_committee_validators(&mut self, ctx: &Ctx, block_id: BlockId) -> usize {
        let func = self
            .validator_registry_contract
            .function("numCommitteeValidators")
            .unwrap()
            .clone();

        let tx = self.gen_l2_call_tx(
            self.validator_registry_address.unwrap(),
            func.short_signature().to_vec(),
        );

        let res = self.eth_call(ctx, block_id, tx).await;

        func.decode_output(&res).unwrap()[0]
            .clone()
            .into_uint()
            .unwrap()
            .as_usize()
    }

    async fn read_num_committee_attesters(&mut self, ctx: &Ctx, block_id: BlockId) -> usize {
        let func = self
            .attester_registry_contract
            .function("numCommitteeAttesters")
            .unwrap()
            .clone();
        let tx = self.gen_l2_call_tx(
            self.attester_registry_address.unwrap(),
            func.short_signature().to_vec(),
        );

        let res = self.eth_call(ctx, block_id, tx).await;
        func.decode_output(&res).unwrap()[0]
            .clone()
            .into_uint()
            .unwrap()
            .as_usize()
    }

    async fn read_validator(
        &mut self,
        ctx: &Ctx,
        block_id: BlockId,
        node_owner: zksync_types::ethabi::Address,
    ) -> (usize, Vec<u8>, Vec<u8>, bool) {
        let func = self
            .validator_registry_contract
            .function("validators")
            .unwrap()
            .clone();
        let tx = self.gen_l2_call_tx(
            self.validator_registry_address.unwrap(),
            func.encode_input(&[Token::Address(node_owner)]).unwrap(),
        );
        let res = self.eth_call(ctx, block_id, tx).await;
        let tokens = func.decode_output(&res).unwrap();
        (
            tokens[0].clone().into_uint().unwrap().as_usize(),
            tokens[1].clone().into_bytes().unwrap(),
            tokens[2].clone().into_bytes().unwrap(),
            tokens[3].clone().into_bool().unwrap(),
        )
    }

    async fn read_attester(
        &mut self,
        ctx: &Ctx,
        block_id: BlockId,
        node_owner: Address,
    ) -> (usize, Vec<u8>, bool) {
        let func = self
            .attester_registry_contract
            .function("attesters")
            .unwrap()
            .clone();
        let tx = self.gen_l2_call_tx(
            self.attester_registry_address.unwrap(),
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
    ) -> (Address, usize, Vec<u8>) {
        let func = self
            .validator_registry_contract
            .function("committee")
            .unwrap()
            .clone();
        let tx = self.gen_l2_call_tx(
            self.validator_registry_address.unwrap(),
            func.encode_input(&[Token::Uint(zksync_types::U256::from(idx))])
                .unwrap(),
        );

        let res = self.eth_call(ctx, block_id, tx).await;
        let tokens = func.decode_output(&res).unwrap();
        (
            tokens[0].clone().into_address().unwrap(),
            tokens[1].clone().into_uint().unwrap().as_usize(),
            tokens[2].clone().into_bytes().unwrap(),
        )
    }

    async fn read_committee_attester(
        &mut self,
        ctx: &Ctx,
        block_id: BlockId,
        idx: usize,
    ) -> (usize, Address, Vec<u8>) {
        let func = self
            .attester_registry_contract
            .function("committee")
            .unwrap()
            .clone();
        let tx = self.gen_l2_call_tx(
            self.attester_registry_address.unwrap(),
            func.encode_input(&[Token::Uint(zksync_types::U256::from(idx))])
                .unwrap(),
        );

        let res = self.eth_call(ctx, block_id, tx).await;
        let tokens = func.decode_output(&res).unwrap();
        (
            tokens[0].clone().into_uint().unwrap().as_usize(),
            tokens[1].clone().into_address().unwrap(),
            tokens[2].clone().into_bytes().unwrap(),
        )
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
    pub node_owner: Address,
    pub weight: usize,
    pub pub_key: Vec<u8>,
}
