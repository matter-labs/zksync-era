use once_cell::sync::Lazy;
pub use zksync_contracts::test_contracts::LoadnextContractExecutionParams as LoadTestParams;
use zksync_contracts::{deployer_contract, TestContract};
use zksync_multivm::utils::get_max_gas_per_pubdata_byte;
use zksync_types::{
    ethabi::{encode, Token},
    fee::Fee,
    l2::L2Tx,
    utils::deployed_address_create,
    Address, K256PrivateKey, L2ChainId, Nonce, ProtocolVersionId, Transaction,
    CONTRACT_DEPLOYER_ADDRESS, H256, U256,
};
use zksync_utils::bytecode::hash_bytecode;

const LOAD_TEST_MAX_READS: usize = 100;

pub(crate) static PRIVATE_KEY: Lazy<K256PrivateKey> =
    Lazy::new(|| K256PrivateKey::from_bytes(H256([42; 32])).expect("invalid key bytes"));
static LOAD_TEST_CONTRACT_ADDRESS: Lazy<Address> =
    Lazy::new(|| deployed_address_create(PRIVATE_KEY.address(), 0.into()));

static LOAD_TEST_CONTRACT: Lazy<TestContract> = Lazy::new(zksync_contracts::get_loadnext_contract);

static CREATE_FUNCTION_SIGNATURE: Lazy<[u8; 4]> = Lazy::new(|| {
    deployer_contract()
        .function("create")
        .unwrap()
        .short_signature()
});

pub fn get_deploy_tx(code: &[u8]) -> Transaction {
    get_deploy_tx_with_gas_limit(code, 30_000_000, 0)
}

pub fn get_deploy_tx_with_gas_limit(code: &[u8], gas_limit: u32, nonce: u32) -> Transaction {
    let mut salt = vec![0_u8; 32];
    salt[28..32].copy_from_slice(&nonce.to_be_bytes());
    let params = [
        Token::FixedBytes(salt),
        Token::FixedBytes(hash_bytecode(code).0.to_vec()),
        Token::Bytes([].to_vec()),
    ];
    let calldata = CREATE_FUNCTION_SIGNATURE
        .iter()
        .cloned()
        .chain(encode(&params))
        .collect();

    let mut signed = L2Tx::new_signed(
        CONTRACT_DEPLOYER_ADDRESS,
        calldata,
        Nonce(nonce),
        tx_fee(gas_limit),
        U256::zero(),
        L2ChainId::from(270),
        &PRIVATE_KEY,
        vec![code.to_vec()], // maybe not needed?
        Default::default(),
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());
    signed.into()
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
        PRIVATE_KEY.address(),
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

pub fn get_load_test_deploy_tx() -> Transaction {
    let calldata = [Token::Uint(LOAD_TEST_MAX_READS.into())];
    let params = [
        Token::FixedBytes(vec![0_u8; 32]),
        Token::FixedBytes(hash_bytecode(&LOAD_TEST_CONTRACT.bytecode).0.to_vec()),
        Token::Bytes(encode(&calldata)),
    ];
    let create_calldata = CREATE_FUNCTION_SIGNATURE
        .iter()
        .cloned()
        .chain(encode(&params))
        .collect();

    let mut factory_deps = LOAD_TEST_CONTRACT.factory_deps.clone();
    factory_deps.push(LOAD_TEST_CONTRACT.bytecode.clone());

    let mut signed = L2Tx::new_signed(
        CONTRACT_DEPLOYER_ADDRESS,
        create_calldata,
        Nonce(0),
        tx_fee(100_000_000),
        U256::zero(),
        L2ChainId::from(270),
        &PRIVATE_KEY,
        factory_deps,
        Default::default(),
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());
    signed.into()
}

pub fn get_load_test_tx(nonce: u32, gas_limit: u32, params: LoadTestParams) -> Transaction {
    assert!(
        params.reads <= LOAD_TEST_MAX_READS,
        "Too many reads: {params:?}, should be <={LOAD_TEST_MAX_READS}"
    );

    let execute_function = LOAD_TEST_CONTRACT
        .contract
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
        *LOAD_TEST_CONTRACT_ADDRESS,
        calldata,
        Nonce(nonce),
        tx_fee(gas_limit),
        U256::zero(),
        L2ChainId::from(270),
        &PRIVATE_KEY,
        LOAD_TEST_CONTRACT.factory_deps.clone(),
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
            reads: 30,
            initial_writes: 2,
            repeated_writes: 9,
            events: 5,
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
            reads: 100,
            initial_writes: 5,
            repeated_writes: 23,
            events: 20,
            hashes: 100,
            recursive_calls: 20,
            deploys: 5,
        },
    )
}
