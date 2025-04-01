use ethabi::Token;
use zksync_eth_signer::{PrivateKeySigner, TransactionParameters};
use zksync_system_constants::{
    DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE,
};
use zksync_types::{
    abi, address_to_u256, bytecode::BytecodeHash, fee::Fee, l2::L2Tx,
    transaction_request::TransactionRequest, utils::deployed_address_create, Address, Execute,
    K256PrivateKey, L2ChainId, Nonce, PackedEthSignature, Transaction, EIP_1559_TX_TYPE, H256,
    PRIORITY_OPERATION_L2_TX_TYPE, U256,
};

pub use self::contracts::{LoadnextContractExecutionParams, TestContract, TestEvmContract};

mod contracts;

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
    // Since transactions produced by the account are ordered by `nonce`, `deploy_nonce` is well-defined as well.
    pub deploy_nonce: Nonce,
}

impl Account {
    pub fn new(private_key: K256PrivateKey) -> Self {
        let address = private_key.address();
        Self {
            private_key,
            address,
            nonce: Nonce(0),
            deploy_nonce: Nonce(0),
        }
    }

    pub fn random() -> Self {
        Self::new(K256PrivateKey::random())
    }

    pub fn random_using(rng: &mut impl rand::Rng) -> Self {
        Self::new(K256PrivateKey::random_using(rng))
    }

    /// Creates an account deterministically from the provided seed.
    pub fn from_seed(seed: u32) -> Self {
        let private_key_bytes = H256::from_low_u64_be(u64::from(seed) + 1);
        Self::new(K256PrivateKey::from_bytes(private_key_bytes).unwrap())
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

    pub fn get_evm_deploy_tx(
        &mut self,
        init_bytecode: Vec<u8>,
        contract: &ethabi::Contract,
        args: &[Token],
    ) -> L2Tx {
        let fee = Self::default_fee();
        let input = if let Some(constructor) = contract.constructor() {
            constructor
                .encode_input(init_bytecode, args)
                .expect("cannot encode constructor args")
        } else {
            assert!(args.is_empty(), "no constructor args expected");
            init_bytecode
        };

        let tx_request = TransactionRequest {
            nonce: self.nonce.0.into(),
            from: Some(self.address),
            to: None,
            value: 0.into(),
            gas_price: fee.max_fee_per_gas,
            gas: fee.gas_limit,
            max_priority_fee_per_gas: Some(fee.max_priority_fee_per_gas),
            input: input.into(),
            // EVM deployment txs must not have the default type (EIP-712).
            transaction_type: Some(EIP_1559_TX_TYPE.into()),
            chain_id: Some(L2ChainId::default().as_u64()),
            ..TransactionRequest::default()
        };

        let data = tx_request
            .get_default_signed_message()
            .expect("no message to sign");
        let sig = PackedEthSignature::sign_raw(&self.private_key, &data).unwrap();
        let raw = tx_request.get_signed_bytes(&sig).unwrap();
        let (req, hash) = TransactionRequest::from_bytes_unverified(&raw).unwrap();
        let mut tx = L2Tx::from_request(req, usize::MAX, true).unwrap();
        tx.set_input(raw, hash);
        tx
    }

    pub fn default_fee() -> Fee {
        Fee {
            gas_limit: U256::from(2_000_000_000u32),
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
        factory_deps: Vec<Vec<u8>>,
        tx_type: TxType,
    ) -> DeployContractsTx {
        let calldata = calldata.unwrap_or_default();
        let code_hash = BytecodeHash::for_bytecode(code).value();
        let mut execute = Execute::for_deploy(H256::zero(), code.to_vec(), calldata);
        execute.factory_deps.extend(factory_deps);

        let tx = match tx_type {
            TxType::L2 => self.get_l2_tx_for_execute(execute, None),
            TxType::L1 { serial_id } => self.get_l1_tx(execute, serial_id),
        };

        let address = deployed_address_create(self.address, self.deploy_nonce.0.into());
        self.deploy_nonce += 1;
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
        let tx = abi::Transaction::L1 {
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
                    .map(|b| BytecodeHash::for_bytecode(b).value_u256())
                    .collect(),
                paymaster_input: vec![],
                reserved_dynamic: vec![],
            }
            .into(),
            factory_deps,
            eth_block: 0,
        };
        Transaction::from_abi(tx, false).unwrap()
    }

    pub fn get_test_contract_transaction(
        &mut self,
        address: Address,
        with_panic: bool,
        value: Option<U256>,
        payable: bool,
        tx_type: TxType,
    ) -> Transaction {
        let test_contract = TestContract::counter();

        let function = if payable {
            test_contract
                .abi
                .function("incrementWithRevertPayable")
                .unwrap()
        } else {
            test_contract.abi.function("incrementWithRevert").unwrap()
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
            factory_deps: if params.deploys == 0 {
                vec![]
            } else {
                TestContract::load_test().factory_deps()
            },
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

    pub fn sign_legacy_tx(&self, tx: TransactionParameters) -> Vec<u8> {
        let pk_signer = self.get_pk_signer();
        pk_signer.sign_transaction(tx)
    }
}
