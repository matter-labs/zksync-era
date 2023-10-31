use ethabi::Token;
use zksync_contracts::test_contracts::LoadnextContractExecutionParams;
use zksync_contracts::{deployer_contract, load_contract};
use zksync_system_constants::{
    CONTRACT_DEPLOYER_ADDRESS, MAX_GAS_PER_PUBDATA_BYTE, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE,
};
use zksync_types::fee::Fee;
use zksync_types::l2::L2Tx;
use zksync_types::utils::deployed_address_create;
use zksync_types::{
    Address, Execute, ExecuteTransactionCommon, L1TxCommonData, L2ChainId, Nonce,
    PackedEthSignature, PriorityOpId, Transaction, H256, U256,
};

use zksync_eth_signer::{raw_ethereum_tx::TransactionParameters, EthereumSigner, PrivateKeySigner};
use zksync_types::l1::{OpProcessingType, PriorityQueueType};

use zksync_utils::bytecode::hash_bytecode;
pub const L1_TEST_GAS_PER_PUBDATA_BYTE: u32 = 800;
const BASE_FEE: u64 = 2_000_000_000;

#[derive(Debug, Clone)]
pub struct DeployContractsTx {
    pub tx: Transaction,
    pub bytecode_hash: H256,
    pub address: Address,
}

#[derive(Debug)]
pub enum TxType {
    L2,
    L1 { serial_id: u64 },
}

#[derive(Debug, Clone)]
pub struct Account {
    private_key: H256,
    pub address: Address,
    pub nonce: Nonce,
}

impl Account {
    pub fn new(private_key: H256) -> Self {
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();
        Self {
            private_key,
            address,
            nonce: Nonce(0),
        }
    }

    pub fn random() -> Self {
        let pk = H256::random();
        Self::new(pk)
    }

    pub fn get_l2_tx_for_execute(&mut self, execute: Execute, fee: Option<Fee>) -> Transaction {
        let tx = self.get_l2_tx_for_execute_with_nonce(execute, fee, self.nonce);
        self.nonce += 1;
        tx
    }

    pub fn get_l2_tx_for_execute_with_nonce(
        &mut self,
        execute: Execute,
        fee: Option<Fee>,
        nonce: Nonce,
    ) -> Transaction {
        let Execute {
            contract_address,
            calldata,
            value,
            factory_deps,
        } = execute;
        let mut tx = L2Tx::new_signed(
            contract_address,
            calldata,
            nonce,
            fee.unwrap_or_else(|| self.default_fee()),
            value,
            L2ChainId::default(),
            &self.private_key,
            factory_deps,
            Default::default(),
        )
        .expect("should create a signed execute transaction");

        tx.set_input(H256::random().0.to_vec(), H256::random());
        tx.into()
    }

    fn default_fee(&self) -> Fee {
        Fee {
            gas_limit: U256::from(2000000000u32),
            max_fee_per_gas: U256::from(BASE_FEE),
            max_priority_fee_per_gas: U256::from(100),
            gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
        }
    }

    pub fn get_deploy_tx(
        &mut self,
        code: &[u8],
        calldata: Option<&[Token]>,
        tx_type: TxType,
    ) -> DeployContractsTx {
        self.get_deploy_tx_with_factory_deps(code, calldata, vec![], tx_type)
    }

    pub fn get_deploy_tx_with_factory_deps(
        &mut self,
        code: &[u8],
        calldata: Option<&[Token]>,
        mut factory_deps: Vec<Vec<u8>>,
        tx_type: TxType,
    ) -> DeployContractsTx {
        let deployer = deployer_contract();

        let contract_function = deployer.function("create").unwrap();

        let calldata = calldata.map(ethabi::encode);
        let code_hash = hash_bytecode(code);
        let params = [
            Token::FixedBytes(vec![0u8; 32]),
            Token::FixedBytes(code_hash.0.to_vec()),
            Token::Bytes(calldata.unwrap_or_default().to_vec()),
        ];
        factory_deps.push(code.to_vec());
        let calldata = contract_function
            .encode_input(&params)
            .expect("failed to encode parameters");

        let execute = Execute {
            contract_address: CONTRACT_DEPLOYER_ADDRESS,
            calldata,
            factory_deps: Some(factory_deps),
            value: U256::zero(),
        };

        let tx = match tx_type {
            TxType::L2 => self.get_l2_tx_for_execute(execute, None),
            TxType::L1 { serial_id } => self.get_l1_tx(execute, serial_id),
        };

        let address =
            // For L1Tx we usually use nonce 0
            deployed_address_create(self.address, (tx.nonce().unwrap_or(Nonce(0)).0).into());
        DeployContractsTx {
            tx,
            bytecode_hash: code_hash,
            address,
        }
    }

    pub fn get_l1_tx(&self, execute: Execute, serial_id: u64) -> Transaction {
        let max_fee_per_gas = U256::from(0u32);
        let gas_limit = U256::from(20_000_000);

        Transaction {
            common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
                sender: self.address,
                gas_limit,
                gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
                to_mint: gas_limit * max_fee_per_gas + execute.value,
                serial_id: PriorityOpId(serial_id),
                max_fee_per_gas,
                canonical_tx_hash: H256::from_low_u64_be(serial_id),
                deadline_block: 100000,
                layer_2_tip_fee: Default::default(),
                op_processing_type: OpProcessingType::Common,
                priority_queue_type: PriorityQueueType::Deque,
                eth_hash: H256::random(),
                eth_block: 1,
                refund_recipient: self.address,
                full_fee: Default::default(),
            }),
            execute,
            received_timestamp_ms: 0,
            raw_bytes: None,
        }
    }

    pub fn get_test_contract_transaction(
        &mut self,
        address: Address,
        with_panic: bool,
        value: Option<U256>,
        payable: bool,
        tx_type: TxType,
    ) -> Transaction {
        let test_contract = load_contract(
            "etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json",
        );

        let function = if payable {
            test_contract
                .function("incrementWithRevertPayable")
                .unwrap()
        } else {
            test_contract.function("incrementWithRevert").unwrap()
        };

        let calldata = function
            .encode_input(&[Token::Uint(U256::from(1u8)), Token::Bool(with_panic)])
            .expect("failed to encode parameters");

        let execute = Execute {
            contract_address: address,
            calldata,
            value: value.unwrap_or_default(),
            factory_deps: None,
        };
        match tx_type {
            TxType::L2 => self.get_l2_tx_for_execute(execute, None),
            TxType::L1 { serial_id } => self.get_l1_tx(execute, serial_id),
        }
    }

    pub fn get_loadnext_transaction(
        &mut self,
        address: Address,
        params: LoadnextContractExecutionParams,
        tx_type: TxType,
    ) -> Transaction {
        let calldata = params.to_bytes();
        let execute = Execute {
            contract_address: address,
            calldata,
            value: U256::zero(),
            factory_deps: None,
        };

        match tx_type {
            TxType::L2 => self.get_l2_tx_for_execute(execute, None),
            TxType::L1 { serial_id } => self.get_l1_tx(execute, serial_id),
        }
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub fn get_pk_signer(&self) -> PrivateKeySigner {
        PrivateKeySigner::new(self.private_key)
    }

    pub async fn sign_legacy_tx(&self, tx: TransactionParameters) -> Vec<u8> {
        let pk_signer = self.get_pk_signer();
        pk_signer.sign_transaction(tx).await.unwrap()
    }
}
