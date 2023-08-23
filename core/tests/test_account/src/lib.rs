#![allow(clippy::upper_case_acronyms)]

// Built-in imports
use std::{fmt, sync::Mutex};

// Workspace uses
use zksync_crypto::rand::{thread_rng, Rng};
use zksync_types::{
    fee::Fee, l2::L2Tx, tx::primitives::PackedEthSignature, Address, Execute, L2ChainId, Nonce,
    CONTRACT_DEPLOYER_ADDRESS, H256, U256,
};
use zksync_utils::bytecode::hash_bytecode;

/// Structure used to sign ZKSync transactions, keeps tracks of its nonce internally
pub struct ZkSyncAccount {
    pub private_key: H256,
    pub address: Address,
    nonce: Mutex<Nonce>,
}

impl Clone for ZkSyncAccount {
    fn clone(&self) -> Self {
        Self {
            private_key: self.private_key,
            address: self.address,
            nonce: Mutex::new(*self.nonce.lock().unwrap()),
        }
    }
}

impl fmt::Debug for ZkSyncAccount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // It is OK to disclose the private key contents for a testkit account.
        f.debug_struct("ZkSyncAccount")
            .field("private_key", &self.private_key)
            .field("address", &self.address)
            .field("nonce", &self.nonce)
            .finish()
    }
}

impl ZkSyncAccount {
    /// Note: probably not secure, use for testing.
    pub fn rand() -> Self {
        let rng = &mut thread_rng();
        let private_key = rng.gen::<[u8; 32]>().into();

        Self::new(private_key, Nonce(0))
    }

    pub fn new(private_key: H256, nonce: Nonce) -> Self {
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();
        Self {
            address,
            private_key,
            nonce: Mutex::new(nonce),
        }
    }

    pub fn nonce(&self) -> Nonce {
        let n = self.nonce.lock().unwrap();
        *n
    }

    pub fn set_nonce(&self, new_nonce: Nonce) {
        *self.nonce.lock().unwrap() = new_nonce;
    }

    pub fn sign_withdraw(
        &self,
        _token: Address,
        _amount: U256,
        _fee: Fee,
        _to: Address,
        _nonce: Option<Nonce>,
        _increment_nonce: bool,
    ) -> L2Tx {
        todo!("New withdrawal support is not yet implemented")

        // let mut stored_nonce = self.nonce.lock().unwrap();
        // let withdraw = GenericL2Tx::<Withdraw>::new_signed(
        //     token,
        //     amount,
        //     to,
        //     nonce.unwrap_or(*stored_nonce),
        //     fee,
        //     L2ChainId(270),
        //     &self.private_key,
        // )
        // .expect("should create a signed transfer transaction");

        // if increment_nonce {
        //     **stored_nonce += 1;
        // }

        // withdraw.into()
    }

    pub fn sign_deploy_contract(
        &self,
        bytecode: Vec<u8>,
        calldata: Vec<u8>,
        fee: Fee,
        nonce: Option<Nonce>,
        increment_nonce: bool,
    ) -> L2Tx {
        let mut stored_nonce = self.nonce.lock().unwrap();
        let bytecode_hash = hash_bytecode(&bytecode);

        let execute_calldata =
            Execute::encode_deploy_params_create(Default::default(), bytecode_hash, calldata);

        let deploy_contract = L2Tx::new_signed(
            CONTRACT_DEPLOYER_ADDRESS,
            execute_calldata,
            nonce.unwrap_or(*stored_nonce),
            fee,
            U256::zero(),
            L2ChainId(270),
            &self.private_key,
            Some(vec![bytecode]),
            Default::default(),
        )
        .expect("should create a signed transfer transaction");

        if increment_nonce {
            **stored_nonce += 1;
        }

        deploy_contract
    }

    pub fn sign_execute(
        &self,
        contract_address: Address,
        calldata: Vec<u8>,
        fee: Fee,
        nonce: Option<Nonce>,
        increment_nonce: bool,
    ) -> L2Tx {
        let mut stored_nonce = self.nonce.lock().unwrap();
        let execute = L2Tx::new_signed(
            contract_address,
            calldata,
            nonce.unwrap_or(*stored_nonce),
            fee,
            U256::zero(),
            L2ChainId(270),
            &self.private_key,
            None,
            Default::default(),
        )
        .expect("should create a signed transfer transaction");

        if increment_nonce {
            **stored_nonce += 1;
        }

        execute
    }

    pub fn get_private_key(&self) -> H256 {
        self.private_key
    }
}
