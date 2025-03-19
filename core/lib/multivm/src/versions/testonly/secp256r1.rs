use zk_evm_1_5_0::zkevm_opcode_defs::p256;
use zksync_system_constants::SECP256R1_VERIFY_PRECOMPILE_ADDRESS;
use zksync_types::{h256_to_u256, web3::keccak256, Execute, H256, U256};

use super::{tester::VmTesterBuilder, TestedVm};
use crate::interface::{ExecutionResult, InspectExecutionMode, TxExecutionMode, VmInterfaceExt};

pub(crate) fn test_secp256r1<VM: TestedVm>() {
    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_execution_mode(TxExecutionMode::EthCall)
        .with_rich_accounts(1)
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];

    // The digest, secret key and public key were copied from the following test suit: `https://github.com/hyperledger/besu/blob/b6a6402be90339367d5bcabcd1cfd60df4832465/crypto/algorithms/src/test/java/org/hyperledger/besu/crypto/SECP256R1Test.java#L36`
    let sk = p256::SecretKey::from_slice(
        &hex::decode("519b423d715f8b581f4fa8ee59f4771a5b44c8130b4e3eacca54a56dda72b464").unwrap(),
    )
    .unwrap();
    let sk = p256::ecdsa::SigningKey::from(sk);

    let digest = keccak256(&hex::decode("5905238877c77421f73e43ee3da6f2d9e2ccad5fc942dcec0cbd25482935faaf416983fe165b1a045ee2bcd2e6dca3bdf46c4310a7461f9a37960ca672d3feb5473e253605fb1ddfd28065b53cb5858a8ad28175bf9bd386a5e471ea7a65c17cc934a9d791e91491eb3754d03799790fe2d308d16146d5c9b0d0debd97d79ce8").unwrap());
    let public_key_encoded = hex::decode("1ccbe91c075fc7f4f033bfa248db8fccd3565de94bbfb12f3c59ff46c271bf83ce4014c68811f9a21a1fdb2c0e6113e06db7ca93b7404e78dc7ccd5ca89a4ca9").unwrap();

    let (sig, _) = sk.sign_prehash_recoverable(&digest).unwrap();
    let (r, s) = sig.split_bytes();

    let mut encoded_r = [0u8; 32];
    encoded_r.copy_from_slice(&r);

    let mut encoded_s = [0u8; 32];
    encoded_s.copy_from_slice(&s);

    let mut x = [0u8; 32];
    x.copy_from_slice(&public_key_encoded[0..32]);

    let mut y = [0u8; 32];
    y.copy_from_slice(&public_key_encoded[32..64]);

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(SECP256R1_VERIFY_PRECOMPILE_ADDRESS),
            calldata: [digest, encoded_r, encoded_s, x, y].concat(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(tx);

    let execution_result = vm.vm.execute(InspectExecutionMode::OneTx);

    let ExecutionResult::Success { output } = execution_result.result else {
        panic!("batch failed")
    };

    let output = H256::from_slice(&output);

    assert_eq!(
        h256_to_u256(output),
        U256::from(1u32),
        "verification was not successful"
    );
}
