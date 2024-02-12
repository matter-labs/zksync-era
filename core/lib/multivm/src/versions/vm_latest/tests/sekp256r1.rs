use std::convert::TryInto;

use zk_evm_1_5_0::zkevm_opcode_defs::p256;
use zksync_system_constants::{L2_ETH_TOKEN_ADDRESS, SEKP_256_R1_PRECOMPILE_ADDRESS};
use zksync_types::{
    get_code_key, get_known_code_key, get_nonce_key,
    system_contracts::{DEPLOYMENT_NONCE_INCREMENT, TX_NONCE_INCREMENT},
    AccountTreeId, Execute, U256,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        tests::{
            tester::{DeployContractsTx, TxType, VmTesterBuilder},
            utils::{get_balance, read_test_contract, verify_required_storage},
        },
        utils::fee::get_batch_base_fee,
        HistoryEnabled,
    },
};

// fn encode_sekp256_r1_calldata() -> Vec<u8> {
// }

#[test]
fn test_sekp256r1() {
    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    // let mut vm = VmTesterBuilder::new(HistoryEnabled)
    //     .with_empty_in_memory_storage()
    //     .with_execution_mode(TxExecutionMode::VerifyExecute)
    //     .with_random_rich_accounts(1)
    //     .build();

    // let counter = read_test_contract();
    // let account = &mut vm.rich_accounts[0];

    // let tx = account.get_l2_tx_for_execute(Execute {
    //     contract_address: SEKP_256_R1_PRECOMPILE_ADDRESS,
    //     calldata: vec[]
    // }, None);

    let sk = p256::SecretKey::from_slice(
        &hex::decode("519b423d715f8b581f4fa8ee59f4771a5b44c8130b4e3eacca54a56dda72b464").unwrap(),
    )
    .unwrap();
    let sk = p256::ecdsa::SigningKey::from(sk);
    let (sig, _) = sk.sign_prehash_recoverable(&hex::decode("5905238877c77421f73e43ee3da6f2d9e2ccad5fc942dcec0cbd25482935faaf416983fe165b1a045ee2bcd2e6dca3bdf46c4310a7461f9a37960ca672d3feb5473e253605fb1ddfd28065b53cb5858a8ad28175bf9bd386a5e471ea7a65c17cc934a9d791e91491eb3754d03799790fe2d308d16146d5c9b0d0debd97d79ce8").unwrap()).unwrap();

    let (r, s) = sig.split_bytes();

    let mut encoded_r = [0u8; 32];
    encoded_r.copy_from_slice(&r);

    let mut encoded_s = [0u8; 32];
    encoded_s.copy_from_slice(&s);

    // println!("{:#?}", sig.r());

    // vm.vm.push_transaction(tx)
}
