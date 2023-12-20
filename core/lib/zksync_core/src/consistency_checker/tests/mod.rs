//! Tests for the consistency checker component.

// FIXME: test checker workflow

use assert_matches::assert_matches;

use super::*;

#[test]
fn extracting_commit_data_for_boojum_batch() {
    let contract = zksync_contracts::zksync_contract();
    let commit_function = contract.function("commitBatches").unwrap();
    // Calldata taken from the commit transaction for https://sepolia.explorer.zksync.io/batch/4470;
    // https://sepolia.etherscan.io/tx/0x300b9115037028b1f8aa2177abf98148c3df95c9b04f95a4e25baf4dfee7711f
    let commit_tx_input_data = include_bytes!("commit_l1_batch_4470_testnet_sepolia.calldata");

    let commit_data = ConsistencyChecker::extract_commit_data(
        commit_tx_input_data,
        commit_function,
        L1BatchNumber(4_470),
    )
    .unwrap();

    assert_matches!(
        commit_data,
        ethabi::Token::Tuple(tuple) if tuple[0] == ethabi::Token::Uint(4_470.into())
    );

    for bogus_l1_batch in [0, 1, 1_000, 4_469, 4_471, 100_000] {
        ConsistencyChecker::extract_commit_data(
            commit_tx_input_data,
            commit_function,
            L1BatchNumber(bogus_l1_batch),
        )
        .unwrap_err();
    }
}

#[test]
fn extracting_commit_data_for_multiple_batches() {
    let contract = zksync_contracts::zksync_contract();
    let commit_function = contract.function("commitBatches").unwrap();
    // Calldata taken from the commit transaction for https://explorer.zksync.io/batch/351000;
    // https://etherscan.io/tx/0xbd8dfe0812df0da534eb95a2d2a4382d65a8172c0b648a147d60c1c2921227fd
    let commit_tx_input_data = include_bytes!("commit_l1_batch_351000-351004_mainnet.calldata");

    for l1_batch in 351_000..=351_004 {
        let commit_data = ConsistencyChecker::extract_commit_data(
            commit_tx_input_data,
            commit_function,
            L1BatchNumber(l1_batch),
        )
        .unwrap();

        assert_matches!(
            commit_data,
            ethabi::Token::Tuple(tuple) if tuple[0] == ethabi::Token::Uint(l1_batch.into())
        );
    }

    for bogus_l1_batch in [350_000, 350_999, 351_005, 352_000] {
        ConsistencyChecker::extract_commit_data(
            commit_tx_input_data,
            commit_function,
            L1BatchNumber(bogus_l1_batch),
        )
        .unwrap_err();
    }
}

#[test]
fn extracting_commit_data_for_pre_boojum_batch() {
    // Calldata taken from the commit transaction for https://goerli.explorer.zksync.io/batch/200000;
    // https://goerli.etherscan.io/tx/0xfd2ef4ccd1223f502cc4a4e0f76c6905feafabc32ba616e5f70257eb968f20a3
    let commit_tx_input_data = include_bytes!("commit_l1_batch_200000_testnet_goerli.calldata");

    let commit_data = ConsistencyChecker::extract_commit_data(
        commit_tx_input_data,
        &PRE_BOOJUM_COMMIT_FUNCTION,
        L1BatchNumber(200_000),
    )
    .unwrap();

    assert_matches!(
        commit_data,
        ethabi::Token::Tuple(tuple) if tuple[0] == ethabi::Token::Uint(200_000.into())
    );
}
