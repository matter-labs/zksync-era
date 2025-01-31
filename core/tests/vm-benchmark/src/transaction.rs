use once_cell::sync::Lazy;
use zksync_multivm::utils::get_max_gas_per_pubdata_byte;
pub use zksync_test_contracts::LoadnextContractExecutionParams as LoadTestParams;
use zksync_test_contracts::{Account, TestContract};
use zksync_types::{
    ethabi::Token, fee::Fee, l2::L2Tx, utils::deployed_address_create, Address, Execute,
    K256PrivateKey, L2ChainId, Nonce, ProtocolVersionId, Transaction, H256, U256,
};

const LOAD_TEST_MAX_READS: usize = 3000;

pub(crate) static PRIVATE_KEY: Lazy<K256PrivateKey> =
    Lazy::new(|| K256PrivateKey::from_bytes(H256([42; 32])).expect("invalid key bytes"));
static LOAD_TEST_CONTRACT_ADDRESS: Lazy<Address> =
    Lazy::new(|| deployed_address_create(PRIVATE_KEY.address(), 0.into()));

pub fn get_deploy_tx(code: &[u8]) -> Transaction {
    get_deploy_tx_with_gas_limit(code, 30_000_000, 0)
}

pub fn get_deploy_tx_with_gas_limit(code: &[u8], gas_limit: u32, nonce: u32) -> Transaction {
    let mut salt = H256::zero();
    salt.0[28..32].copy_from_slice(&nonce.to_be_bytes());
    let execute = Execute::for_deploy(salt, code.to_vec(), &[]);
    let mut account = Account::new(PRIVATE_KEY.clone());
    account.nonce = Nonce(nonce);
    account.get_l2_tx_for_execute(execute, Some(tx_fee(gas_limit)))
}

fn tx_fee(gas_limit: u32) -> Fee {
    Fee {
        gas_limit: U256::from(gas_limit),
        max_fee_per_gas: U256::from(250_000_000),
        max_priority_fee_per_gas: U256::from(0),
        gas_per_pubdata_limit: U256::from(get_max_gas_per_pubdata_byte(
            ProtocolVersionId::latest().into(),
        )),
    }
}

pub fn get_transfer_tx(nonce: u32) -> Transaction {
    let mut signed = L2Tx::new_signed(
        Some(PRIVATE_KEY.address()),
        vec![], // calldata
        Nonce(nonce),
        tx_fee(1_000_000),
        1_000_000_000.into(), // value
        L2ChainId::from(270),
        &PRIVATE_KEY,
        vec![],             // factory deps
        Default::default(), // paymaster params
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());
    signed.into()
}

pub fn get_erc20_transfer_tx(nonce: u32) -> Transaction {
    let transfer_fn = TestContract::test_erc20().function("transfer");
    let calldata = transfer_fn
        .encode_input(&[
            Token::Address(Address::from_low_u64_be(nonce.into())), // send tokens to unique addresses
            Token::Uint(1.into()),
        ])
        .unwrap();

    let mut signed = L2Tx::new_signed(
        Some(*LOAD_TEST_CONTRACT_ADDRESS),
        calldata,
        Nonce(nonce),
        tx_fee(1_000_000),
        0.into(), // value
        L2ChainId::from(270),
        &PRIVATE_KEY,
        vec![],             // factory deps
        Default::default(), // paymaster params
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());
    signed.into()
}

pub fn get_erc20_deploy_tx() -> Transaction {
    let calldata = [Token::Uint(U256::one() << 128)]; // initial token amount minted to the deployer
    let execute = TestContract::test_erc20().deploy_payload(&calldata);
    Account::new(PRIVATE_KEY.clone()).get_l2_tx_for_execute(execute, Some(tx_fee(500_000_000)))
}

pub fn get_load_test_deploy_tx() -> Transaction {
    let calldata = [Token::Uint(LOAD_TEST_MAX_READS.into())];
    let execute = TestContract::load_test().deploy_payload(&calldata);
    Account::new(PRIVATE_KEY.clone()).get_l2_tx_for_execute(execute, Some(tx_fee(500_000_000)))
}

pub fn get_load_test_tx(nonce: u32, gas_limit: u32, params: LoadTestParams) -> Transaction {
    assert!(
        params.reads <= LOAD_TEST_MAX_READS,
        "Too many reads: {params:?}, should be <={LOAD_TEST_MAX_READS}"
    );

    let execute_function = TestContract::load_test()
        .abi
        .function("execute")
        .expect("no `execute` function in load test contract");
    let calldata = execute_function
        .encode_input(&vec![
            Token::Uint(U256::from(params.reads)),
            Token::Uint(U256::from(params.initial_writes)),
            Token::Uint(U256::from(params.repeated_writes)),
            Token::Uint(U256::from(params.hashes)),
            Token::Uint(U256::from(params.events)),
            Token::Uint(U256::from(params.recursive_calls)),
            Token::Uint(U256::from(params.deploys)),
        ])
        .expect("cannot encode `execute` inputs");

    let mut signed = L2Tx::new_signed(
        Some(*LOAD_TEST_CONTRACT_ADDRESS),
        calldata,
        Nonce(nonce),
        tx_fee(gas_limit),
        U256::zero(),
        L2ChainId::from(270),
        &PRIVATE_KEY,
        TestContract::load_test().factory_deps(),
        Default::default(),
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());
    signed.into()
}

pub fn get_realistic_load_test_tx(nonce: u32) -> Transaction {
    get_load_test_tx(
        nonce,
        10_000_000,
        LoadTestParams {
            reads: 243,
            initial_writes: 1,
            repeated_writes: 11,
            events: 6,
            hashes: 10,
            recursive_calls: 0,
            deploys: 0,
        },
    )
}

pub fn get_heavy_load_test_tx(nonce: u32) -> Transaction {
    get_load_test_tx(
        nonce,
        10_000_000,
        LoadTestParams {
            reads: 296,
            initial_writes: 13,
            repeated_writes: 92,
            events: 140,
            hashes: 100,
            recursive_calls: 20,
            deploys: 5,
        },
    )
}
