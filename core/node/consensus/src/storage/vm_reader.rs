use std::time::Duration;

use anyhow::Context;
use zksync_basic_types::{
    ethabi::Bytes,
    web3::contract::{Detokenize, Error, Tokenizable, Tokenize},
};
use zksync_concurrency::ctx::Ctx;
use zksync_contracts::load_contract;
use zksync_node_api_server::{
    execution_sandbox::{BlockArgs, BlockStartInfo},
    tx_sender::TxSender,
};
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    api::BlockId,
    ethabi::{Address, Contract, Function, Token},
    fee::Fee,
    l2::L2Tx,
    transaction_request::CallOverrides,
    Nonce, U256,
};
use zksync_web3_decl::types::H160;

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

    pub async fn read_committees(
        &self,
        ctx: &Ctx,
        block_id: BlockId,
    ) -> anyhow::Result<(Vec<CommitteeValidator>, Vec<CommitteeAttester>)> {
        let mut conn = self.pool.connection(ctx).await.unwrap().0;
        let start_info = BlockStartInfo::new(&mut conn, Duration::from_secs(10))
            .await
            .unwrap();
        let block_args = BlockArgs::new(&mut conn, block_id, &start_info)
            .await
            .unwrap();

        let validator_committee = self
            .read_validator_committee(block_args)
            .await
            .context("read_validator_committee()")?;
        let attester_committee = self
            .read_attester_committee(block_args)
            .await
            .context("read_attester_committee()")?;

        Ok((validator_committee, attester_committee))
    }

    pub async fn read_validator_committee(
        &self,
        block_args: BlockArgs,
    ) -> anyhow::Result<Vec<CommitteeValidator>> {
        let mut committee = vec![];
        let validator_committee_size = self
            .read_validator_committee_size(block_args)
            .await
            .context("read_validator_committee_size()")?;
        for i in 0..validator_committee_size {
            let committee_validator = self
                .read_committee_validator(block_args, i)
                .await
                .context("read_committee_validator()")?;
            committee.push(committee_validator)
        }
        Ok(committee)
    }

    pub async fn read_attester_committee(
        &self,
        block_args: BlockArgs,
    ) -> anyhow::Result<Vec<CommitteeAttester>> {
        let mut committee = vec![];
        let attester_committee_size = self
            .read_attester_committee_size(block_args)
            .await
            .context("read_attester_committee_size()")?;
        for i in 0..attester_committee_size {
            let committee_validator = self
                .read_committee_attester(block_args, i)
                .await
                .context("read_committee_attester()")?;
            committee.push(committee_validator)
        }
        Ok(committee)
    }

    async fn read_validator_committee_size(&self, block_args: BlockArgs) -> anyhow::Result<usize> {
        let func = self
            .registry_contract
            .function("validatorCommitteeSize")
            .unwrap()
            .clone();

        let tx = self.gen_l2_call_tx(self.registry_address, func.short_signature().to_vec());

        let res = self.eth_call(block_args, tx).await;

        let tokens = func.decode_output(&res).context("decode_output()")?;
        U256::from_tokens(tokens)
            .context("U256::from_tokens()")
            .map(|t| t.as_usize())
    }

    async fn read_attester_committee_size(&self, block_args: BlockArgs) -> anyhow::Result<usize> {
        let func = self
            .registry_contract
            .function("attesterCommitteeSize")
            .unwrap()
            .clone();
        let tx = self.gen_l2_call_tx(self.registry_address, func.short_signature().to_vec());

        let res = self.eth_call(block_args, tx).await;
        let tokens = func.decode_output(&res).context("decode_output()")?;
        U256::from_tokens(tokens)
            .context("U256::from_tokens()")
            .map(|t| t.as_usize())
    }

    async fn read_committee_validator(
        &self,
        block_args: BlockArgs,
        idx: usize,
    ) -> anyhow::Result<CommitteeValidator> {
        let func = self
            .registry_contract
            .function("validatorCommittee")
            .unwrap()
            .clone();
        let tx = self.gen_l2_call_tx(
            self.registry_address,
            func.encode_input(&zksync_types::U256::from(idx).into_tokens())
                .unwrap(),
        );

        let res = self.eth_call(block_args, tx).await;
        let tokens = func.decode_output(&res).context("decode_output()")?;
        CommitteeValidator::from_tokens(tokens).context("CommitteeValidator::from_tokens()")
    }

    async fn read_committee_attester(
        &self,
        block_args: BlockArgs,
        idx: usize,
    ) -> anyhow::Result<CommitteeAttester> {
        let func = self
            .registry_contract
            .function("attesterCommittee")
            .unwrap()
            .clone();

        let tx = self.gen_l2_call_tx(
            self.registry_address,
            func.encode_input(&zksync_types::U256::from(idx).into_tokens())
                .unwrap(),
        );

        let res = self.eth_call(block_args, tx).await;
        let tokens = func.decode_output(&res).context("decode_output()")?;
        CommitteeAttester::from_tokens(tokens).context("CommitteeAttester::from_tokens()")
    }

    async fn eth_call(&self, block_args: BlockArgs, tx: L2Tx) -> Vec<u8> {
        let call_overrides = CallOverrides {
            enforced_base_fee: None,
        };
        self.tx_sender
            .eth_call(block_args, call_overrides, tx)
            .await
            .unwrap()
    }

    fn gen_l2_call_tx(&self, contract_address: Address, calldata: Vec<u8>) -> L2Tx {
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
pub struct CommitteeAttester {
    pub weight: usize,
    pub node_owner: Address,
    pub pub_key: Vec<u8>,
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
