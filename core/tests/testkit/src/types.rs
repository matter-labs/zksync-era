//! Common primitives used within testkit.
#![allow(clippy::map_entry)] // Clippy doesn't take `async` code in block into account.

use std::collections::{HashMap, VecDeque};
use std::default::Default;

use num::{bigint::Sign, BigUint, Zero};
use zksync_types::web3::transports::Http;
use zksync_types::L1ChainId;

use zksync_config::ZkSyncConfig;
// use zksync_crypto::franklin_crypto::bellman::pairing::{
//     bn256::Fr,
//     ff::{PrimeField, PrimeFieldRepr},
// };
// use zksync_prover_utils::precomputed::load_precomputed_proofs;
use zksync_test_account::ZkSyncAccount;
use zksync_types::aggregated_operations::{
    BlocksCommitOperation, BlocksExecuteOperation, BlocksProofOperation,
};
use zksync_types::{
    api, commitment::BlockWithMetadata, AccountTreeId, Address, L1BatchNumber, Nonce, H256,
};
use zksync_utils::u256_to_biguint;

use crate::eth_provider::EthereumProvider;
use crate::utils::is_token_eth;
use zksync_dal::StorageProcessor;

pub static ETHEREUM_ADDRESS: Address = Address::zero();
pub const VERY_BIG_BLOCK_NUMBER: L1BatchNumber = L1BatchNumber(10000000);

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum BlockProcessing {
    CommitAndExecute,
    CommitAndRevert,
}

#[derive(Debug, Clone)]
pub struct AccountHandler {
    pub sync_account: ZkSyncAccount,
    pub eth_provider: EthereumProvider,
}

impl AccountHandler {
    pub fn new(
        private_key: H256,
        transport: Http,
        config: &ZkSyncConfig,
        zksync_contract: Address,
    ) -> Self {
        let sync_account = ZkSyncAccount::new(private_key, Nonce(0));
        let eth_provider = EthereumProvider::new(
            sync_account.private_key,
            transport,
            zksync_contract,
            L1ChainId(config.eth_client.chain_id),
        );

        Self {
            sync_account,
            eth_provider,
        }
    }

    pub fn rand(config: &ZkSyncConfig, zksync_contract: Address) -> Self {
        let sync_account = ZkSyncAccount::rand();
        let eth_provider = {
            let transport = Http::new(&config.eth_client.web3_url).expect("http transport start");
            EthereumProvider::new(
                sync_account.private_key,
                transport,
                zksync_contract,
                L1ChainId(config.eth_client.chain_id),
            )
        };

        Self {
            sync_account,
            eth_provider,
        }
    }

    pub fn address(&self) -> Address {
        self.eth_provider.address
    }
}

#[derive(PartialEq)]
pub enum LayerType {
    Ethereum,
    Zksync,
}

pub struct BalanceUpdate {
    layer_type: LayerType,
    sign: Sign,
    amount: BigUint,
}

impl BalanceUpdate {
    pub fn new(layer_type: LayerType, sign: Sign, amount: BigUint) -> Self {
        Self {
            layer_type,
            sign,
            amount,
        }
    }
}

// Struct used to keep expected balance changes after transactions execution.
#[derive(Debug)]
pub struct AccountBalances {
    operator_account: AccountHandler,

    /// (Account address, Token address) -> balance
    pub eth_accounts_balances: HashMap<(Address, Address), BigUint>,
    /// (Account address, Token address) -> balance
    pub sync_accounts_balances: HashMap<(Address, Address), BigUint>,
}

impl AccountBalances {
    pub fn new(operator_account: AccountHandler) -> Self {
        Self {
            operator_account,
            eth_accounts_balances: Default::default(),
            sync_accounts_balances: Default::default(),
        }
    }

    pub async fn get_real_balance(
        &self,
        storage: &mut StorageProcessor<'static>,
        account: Address,
        token: Address,
        layer_type: LayerType,
    ) -> BigUint {
        if layer_type == LayerType::Ethereum {
            let balances_to_withdraw = self
                .operator_account
                .eth_provider
                .balances_to_withdraw(token, Some(account))
                .await
                .unwrap();

            let main_balance = if is_token_eth(token) {
                self.operator_account
                    .eth_provider
                    .eth_balance(Some(account))
                    .await
                    .unwrap()
            } else {
                self.operator_account
                    .eth_provider
                    .erc20_balance(&token, Some(account))
                    .await
                    .unwrap()
            };

            main_balance + balances_to_withdraw
        } else {
            let balance = storage
                .storage_web3_dal()
                .standard_token_historical_balance(
                    AccountTreeId::new(token),
                    AccountTreeId::new(account),
                    api::BlockId::Number(api::BlockNumber::Pending),
                )
                .unwrap()
                .unwrap();
            u256_to_biguint(balance)
        }
    }

    pub async fn setup_balances(
        &mut self,
        storage: &mut StorageProcessor<'static>,
        accounts_and_tokens: &[(AccountHandler, Address)],
    ) {
        for (account_handler, token_address) in accounts_and_tokens {
            if account_handler.address() == self.operator_account.address() {
                continue;
            }

            // Add info for L1 balance
            if !self
                .eth_accounts_balances
                .contains_key(&(account_handler.address(), *token_address))
            {
                let balance = self
                    .get_real_balance(
                        storage,
                        account_handler.address(),
                        *token_address,
                        LayerType::Ethereum,
                    )
                    .await;

                self.eth_accounts_balances
                    .insert((account_handler.address(), *token_address), balance);
            }

            // Add info for L2 balance
            if !self
                .sync_accounts_balances
                .contains_key(&(account_handler.address(), *token_address))
            {
                let balance = self
                    .get_real_balance(
                        storage,
                        account_handler.address(),
                        *token_address,
                        LayerType::Zksync,
                    )
                    .await;

                self.sync_accounts_balances
                    .insert((account_handler.address(), *token_address), balance);
            }
        }
    }

    pub fn update_balance(
        &mut self,
        account: Address,
        token: Address,
        balance_update: BalanceUpdate,
    ) {
        if account != self.operator_account.address() {
            let balance = match balance_update.layer_type {
                LayerType::Ethereum => self.eth_accounts_balances.get_mut(&(account, token)),
                LayerType::Zksync => self.sync_accounts_balances.get_mut(&(account, token)),
            }
            .unwrap();
            match balance_update.sign {
                Sign::Plus => {
                    *balance += balance_update.amount;
                }
                Sign::Minus => {
                    *balance -= balance_update.amount;
                }
                Sign::NoSign => {
                    assert!(balance_update.amount.is_zero());
                }
            }
        }
    }
}

#[derive(Default)]
pub struct OperationsQueue {
    commit_queue: VecDeque<BlocksCommitOperation>,
    verify_queue: VecDeque<BlocksProofOperation>,
    execute_queue: VecDeque<BlocksExecuteOperation>,
}

impl OperationsQueue {
    pub fn add_commit_op(&mut self, commit_op: BlocksCommitOperation) {
        self.commit_queue.push_back(commit_op);
    }

    pub fn get_commit_op(&mut self) -> Option<BlocksCommitOperation> {
        let commit_op = self.commit_queue.pop_front();
        if let Some(ref commit_op) = commit_op {
            // let mut proof = load_precomputed_proofs().unwrap().aggregated_proof;
            // proof.individual_vk_inputs = Vec::new();
            // for block in &commit_op.blocks {
            //     let mut block_commitment = block.block_commitment.as_bytes().to_vec();
            //     block_commitment[0] &= 0xffu8 >> 3;
            //
            //     //convert from bytes to fr
            //     let mut fr_repr = <Fr as PrimeField>::Repr::default();
            //     fr_repr.read_be(&*block_commitment).unwrap();
            //     let block_commitment = Fr::from_repr(fr_repr).unwrap();
            //
            //     proof.individual_vk_inputs.push(block_commitment);
            //     proof.individual_vk_idxs.push(0);
            // }

            let verify_op = BlocksProofOperation {
                // This should be changed if testkit is be restored as it is not a previous block.
                prev_block: commit_op.blocks.first().unwrap().clone(),
                blocks: commit_op.blocks.clone(),
                proofs: Vec::default(),
                should_verify: false,
            };

            self.verify_queue.push_back(verify_op);
        }
        commit_op
    }

    pub fn get_verify_op(&mut self) -> Option<BlocksProofOperation> {
        let verify_op = self.verify_queue.pop_front();
        if let Some(ref verify_op) = verify_op {
            let execute_op = BlocksExecuteOperation {
                blocks: verify_op.blocks.clone(),
            };

            self.execute_queue.push_back(execute_op);
        }
        verify_op
    }

    pub fn get_execute_op(&mut self) -> Option<BlocksExecuteOperation> {
        self.execute_queue.pop_front()
    }

    pub fn revert_blocks(&mut self) -> Option<Vec<BlockWithMetadata>> {
        // the block that needs to be reverted is already committed,
        // so it is in the queue for verification.
        self.verify_queue
            .pop_front()
            .map(|verify_op| verify_op.blocks)
    }
}
