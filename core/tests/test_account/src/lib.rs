use ethabi::Token;
use zksync_contracts::{
    deployer_contract, load_contract, test_contracts::LoadnextContractExecutionParams,
};
use zksync_eth_signer::{EthereumSigner, PrivateKeySigner, TransactionParameters};
use zksync_system_constants::{
    CONTRACT_DEPLOYER_ADDRESS, DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE,
    REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE,
};
use zksync_types::{
    abi, fee::Fee, l2::L2Tx, utils::deployed_address_create, Address, Execute, K256PrivateKey,
    L2ChainId, Nonce, Transaction, H256, PRIORITY_OPERATION_L2_TX_TYPE, U256,
};
use zksync_utils::{address_to_u256, bytecode::hash_bytecode, h256_to_u256};

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
    private_key: K256PrivateKey,
    pub address: Address,
    pub nonce: Nonce,
}

impl Account {
    pub fn new(private_key: K256PrivateKey) -> Self {
        let address = private_key.address();
        Self {
            private_key,
            address,
            nonce: Nonce(0),
        }
    }

    pub fn random() -> Self {
        Self::new(K256PrivateKey::random())
    }

    pub fn random_using(rng: &mut impl rand::Rng) -> Self {
        Self::new(K256PrivateKey::random_using(rng))
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
        L2Tx::new_signed(
            contract_address,
            calldata,
            nonce,
            fee.unwrap_or_else(Self::default_fee),
            value,
            L2ChainId::default(),
            &self.private_key,
            factory_deps,
            Default::default(),
        )
        .expect("should create a signed execute transaction")
        .into()
    }

    pub fn default_fee() -> Fee {
        Fee {
            gas_limit: U256::from(2000000000u32),
            max_fee_per_gas: U256::from(BASE_FEE),
            max_priority_fee_per_gas: U256::from(100),
            gas_per_pubdata_limit: U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE),
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
            contract_address: Some(CONTRACT_DEPLOYER_ADDRESS),
            calldata,
            factory_deps,
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
        let factory_deps = execute.factory_deps;
        abi::Transaction::L1 {
            tx: abi::L2CanonicalTransaction {
                tx_type: PRIORITY_OPERATION_L2_TX_TYPE.into(),
                from: address_to_u256(&self.address),
                to: address_to_u256(&execute.contract_address.unwrap_or_default()),
                gas_limit,
                gas_per_pubdata_byte_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
                max_fee_per_gas,
                max_priority_fee_per_gas: 0.into(),
                paymaster: 0.into(),
                nonce: serial_id.into(),
                value: execute.value,
                reserved: [
                    // `to_mint`
                    gas_limit * max_fee_per_gas + execute.value,
                    // `refund_recipient`
                    address_to_u256(&self.address),
                    0.into(),
                    0.into(),
                ],
                data: execute.calldata,
                signature: vec![],
                factory_deps: factory_deps
                    .iter()
                    .map(|b| h256_to_u256(hash_bytecode(b)))
                    .collect(),
                paymaster_input: vec![],
                reserved_dynamic: vec![],
            }
            .into(),
            factory_deps,
            eth_block: 0,
        }
        .try_into()
        .unwrap()
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
            contract_address: Some(address),
            calldata,
            value: value.unwrap_or_default(),
            factory_deps: vec![],
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
            contract_address: Some(address),
            calldata,
            value: U256::zero(),
            factory_deps: vec![],
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
        PrivateKeySigner::new(self.private_key.clone())
    }

    pub async fn sign_legacy_tx(&self, tx: TransactionParameters) -> Vec<u8> {
        let pk_signer = self.get_pk_signer();
        pk_signer.sign_transaction(tx).await.unwrap()
    }
}
