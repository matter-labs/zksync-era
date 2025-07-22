use std::{
    collections::{HashMap, HashSet},
    iter::FromIterator,
};

use zksync_types::{
    fee::Fee,
    helpers::unix_timestamp_ms,
    l1::{OpProcessingType, PriorityQueueType},
    l2::L2Tx,
    Address, Execute, ExecuteTransactionCommon, L1TxCommonData, Nonce, PriorityOpId,
    ProtocolVersionId, Transaction, TransactionTimeRangeConstraint, H256, U256,
};

use crate::{mempool_store::MempoolStore, types::L2TxFilter, AdvanceInput};

#[test]
fn basic_flow() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100, None, None);
    let account0 = Address::random();
    let account1 = Address::random();
    let transactions = vec![
        gen_l2_tx(account0, Nonce(0)),
        gen_l2_tx(account0, Nonce(1)),
        gen_l2_tx(account0, Nonce(2)),
        gen_l2_tx(account0, Nonce(3)),
        gen_l2_tx(account1, Nonce(1)),
    ];
    assert_eq!(mempool.next_transaction(&L2TxFilter::default()), None);
    mempool.insert_without_constraints(transactions, HashMap::new());
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account0, 0)
    );
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account0, 1)
    );
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account0, 2)
    );
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account0, 3)
    );
    assert_eq!(mempool.next_transaction(&L2TxFilter::default()), None);
    // unclog second account and insert more transactions
    mempool.insert_without_constraints(
        vec![gen_l2_tx(account1, Nonce(0)), gen_l2_tx(account0, Nonce(3))],
        HashMap::new(),
    );
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account1, 0)
    );
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account1, 1)
    );
    assert_eq!(mempool.next_transaction(&L2TxFilter::default()), None);
}

#[test]
fn missing_txns() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100, None, None);
    let account = Address::random();
    let transactions = vec![
        gen_l2_tx(account, Nonce(6)),
        gen_l2_tx(account, Nonce(7)),
        gen_l2_tx(account, Nonce(9)),
    ];
    let mut nonces = HashMap::new();
    nonces.insert(account, Nonce(5));
    mempool.insert_without_constraints(transactions, nonces);
    assert_eq!(mempool.next_transaction(&L2TxFilter::default()), None);
    // missing transaction unclogs mempool
    mempool.insert_without_constraints(vec![gen_l2_tx(account, Nonce(5))], HashMap::new());
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account, 5)
    );
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account, 6)
    );
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account, 7)
    );

    // filling remaining gap
    mempool.insert_without_constraints(vec![gen_l2_tx(account, Nonce(8))], HashMap::new());
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account, 8)
    );
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account, 9)
    );
}

#[test]
fn prioritize_l1_txns() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100, None, None);
    let account = Address::random();
    let transactions = vec![
        gen_l2_tx(account, Nonce(0)),
        gen_l2_tx(account, Nonce(1)),
        gen_l1_tx(PriorityOpId(0)),
    ];
    mempool.insert_without_constraints(transactions, HashMap::new());
    assert!(mempool
        .next_transaction(&L2TxFilter::default())
        .unwrap()
        .0
        .is_l1())
}

#[test]
fn l1_txns_priority_id() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100, None, None);
    let transactions = vec![
        gen_l1_tx(PriorityOpId(1)),
        gen_l1_tx(PriorityOpId(2)),
        gen_l1_tx(PriorityOpId(3)),
    ];
    mempool.insert_without_constraints(transactions, HashMap::new());
    assert!(mempool.next_transaction(&L2TxFilter::default()).is_none());
    mempool.insert_without_constraints(vec![gen_l1_tx(PriorityOpId(0))], HashMap::new());
    for idx in 0..4 {
        let data = mempool
            .next_transaction(&L2TxFilter::default())
            .unwrap()
            .0
            .common_data;
        match data {
            ExecuteTransactionCommon::L1(data) => {
                assert_eq!(data.serial_id, PriorityOpId(idx as u64));
            }
            _ => unreachable!("expected L1 transaction"),
        }
    }
}

#[test]
fn rejected_tx() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100, None, None);
    let account = Address::random();
    let transactions = vec![
        gen_l2_tx(account, Nonce(0)),
        gen_l2_tx(account, Nonce(1)),
        gen_l2_tx(account, Nonce(2)),
        gen_l2_tx(account, Nonce(3)),
        gen_l2_tx(account, Nonce(5)),
    ];
    mempool.insert_without_constraints(transactions, HashMap::new());
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account, 0)
    );
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account, 1)
    );

    mempool.rollback(&gen_l2_tx(account, Nonce(1)));
    assert!(mempool.next_transaction(&L2TxFilter::default()).is_none());

    // replace transaction and unblock account
    mempool.insert_without_constraints(vec![gen_l2_tx(account, Nonce(1))], HashMap::new());
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account, 1)
    );
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account, 2)
    );
    assert_eq!(
        view(mempool.next_transaction(&L2TxFilter::default())),
        (account, 3)
    );
}

#[test]
fn replace_tx() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100, None, None);
    let account = Address::random();
    mempool.insert_without_constraints(vec![gen_l2_tx(account, Nonce(0))], HashMap::new());
    // replace it
    mempool.insert_without_constraints(
        vec![gen_l2_tx_with_timestamp(
            account,
            Nonce(0),
            unix_timestamp_ms() + 10,
        )],
        HashMap::new(),
    );
    assert!(mempool.next_transaction(&L2TxFilter::default()).is_some());
    assert!(mempool.next_transaction(&L2TxFilter::default()).is_none());
}

#[test]
fn two_ready_txs() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100, None, None);
    let account0 = Address::random();
    let account1 = Address::random();
    let transactions = vec![gen_l2_tx(account0, Nonce(0)), gen_l2_tx(account1, Nonce(0))];
    mempool.insert_without_constraints(transactions, HashMap::new());
    assert_eq!(
        HashSet::<(_, _)>::from_iter(vec![
            view(mempool.next_transaction(&L2TxFilter::default())),
            view(mempool.next_transaction(&L2TxFilter::default()))
        ]),
        HashSet::<(_, _)>::from_iter(vec![(account0, 0), (account1, 0)]),
    );
}

#[test]
fn mempool_size() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100, None, None);
    let account0 = Address::random();
    let account1 = Address::random();
    let transactions = vec![
        gen_l2_tx(account0, Nonce(0)),
        gen_l2_tx(account0, Nonce(1)),
        gen_l2_tx(account0, Nonce(2)),
        gen_l2_tx(account0, Nonce(3)),
        gen_l2_tx(account1, Nonce(1)),
    ];
    mempool.insert_without_constraints(transactions, HashMap::new());
    assert_eq!(mempool.stats().l2_transaction_count, 5);
    // replacement
    mempool.insert_without_constraints(vec![gen_l2_tx(account0, Nonce(2))], HashMap::new());
    assert_eq!(mempool.stats().l2_transaction_count, 5);
    // load next
    mempool.next_transaction(&L2TxFilter::default());
    mempool.next_transaction(&L2TxFilter::default());
    assert_eq!(mempool.stats().l2_transaction_count, 3);
}

/// Checks whether filtering transactions based on their fee works as expected.
#[test]
fn filtering() {
    // Filter to find transactions with non-zero `gas_per_pubdata` values.
    let filter_non_zero = L2TxFilter {
        fee_input: Default::default(),
        fee_per_gas: 0u64,
        gas_per_pubdata: 1u32,
        protocol_version: ProtocolVersionId::latest(),
    };
    // No-op filter that fetches any transaction.
    let filter_zero = L2TxFilter {
        fee_input: Default::default(),
        fee_per_gas: 0u64,
        gas_per_pubdata: 0u32,
        protocol_version: ProtocolVersionId::latest(),
    };

    let mut mempool = MempoolStore::new(PriorityOpId(0), 100, None, None);
    let account0 = Address::random();
    let account1 = Address::random();

    // First account will have two transactions: one with too low pubdata price and one with the right value.
    // Second account will have just one transaction with the right value.
    mempool.insert_without_constraints(
        gen_transactions_for_filtering(vec![
            (account0, Nonce(0), unix_timestamp_ms(), 0),
            (account0, Nonce(1), unix_timestamp_ms(), 1),
            (account1, Nonce(0), unix_timestamp_ms() - 10, 1),
        ]),
        HashMap::new(),
    );

    // First transaction from first account doesn't match the filter, so we should get the transaction
    // from the second account.
    assert_eq!(
        view(mempool.next_transaction(&filter_non_zero)),
        (account1, 0)
    );
    // No more transactions can be executed with the non-zero filter.
    assert_eq!(mempool.next_transaction(&filter_non_zero), None);

    // Now we apply zero filter and get the transaction as expected.
    assert_eq!(view(mempool.next_transaction(&filter_zero)), (account0, 0));
    assert_eq!(view(mempool.next_transaction(&filter_zero)), (account0, 1));
    assert_eq!(mempool.next_transaction(&filter_zero), None);
}

#[test]
fn stashed_accounts() {
    let filter_non_zero = L2TxFilter {
        fee_input: Default::default(),
        fee_per_gas: 0u64,
        gas_per_pubdata: 1u32,
        protocol_version: ProtocolVersionId::latest(),
    };
    // No-op filter that fetches any transaction.
    let filter_zero = L2TxFilter {
        fee_input: Default::default(),
        fee_per_gas: 0u64,
        gas_per_pubdata: 0u32,
        protocol_version: ProtocolVersionId::latest(),
    };
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100, None, None);
    let account0 = Address::random();
    let account1 = Address::random();

    mempool.insert_without_constraints(
        gen_transactions_for_filtering(vec![
            (account0, Nonce(0), unix_timestamp_ms(), 1),
            (account0, Nonce(1), unix_timestamp_ms(), 0),
            (account0, Nonce(2), unix_timestamp_ms(), 1),
            (account1, Nonce(0), unix_timestamp_ms() + 10, 1),
        ]),
        HashMap::new(),
    );

    // No stashed account at the start.
    assert!(mempool.get_mempool_info().stashed_accounts.is_empty());

    // Get next tx, should return first tx for account0 and not stash any account.
    assert_eq!(
        view(mempool.next_transaction(&filter_non_zero)),
        (account0, 0)
    );
    assert!(mempool.get_mempool_info().stashed_accounts.is_empty());

    // Get next tx, should return first tx for account1 and stash account0.
    assert_eq!(
        view(mempool.next_transaction(&filter_non_zero)),
        (account1, 0)
    );
    assert_eq!(mempool.get_mempool_info().stashed_accounts, vec![account0]);

    // Check that nonce for stashed account was preserved.
    assert_eq!(mempool.account_nonce(account0), Some(Nonce(1)));

    assert!(mempool.next_transaction(&filter_zero).is_none());
}

#[test]
fn mempool_capacity() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 4, None, None);
    let account0 = Address::random();
    let account1 = Address::random();
    let account2 = Address::random();
    let account3 = Address::random();
    let transactions = vec![
        gen_l2_tx(account0, Nonce(0)),
        gen_l2_tx(account0, Nonce(1)),
        gen_l2_tx(account0, Nonce(2)),
        gen_l2_tx_with_timestamp(account1, Nonce(0), unix_timestamp_ms() + 1),
        gen_l2_tx_with_timestamp(account2, Nonce(0), unix_timestamp_ms() + 2),
        gen_l2_tx(account3, Nonce(1)),
    ];
    mempool.insert_without_constraints(transactions, HashMap::new());
    // Mempool is full. Accounts with non-sequential nonces and some accounts with lowest score should be purged.
    assert_eq!(
        HashSet::<_>::from_iter(mempool.get_mempool_info().purged_accounts),
        HashSet::from([account2, account3]),
    );
    // verify that good-to-go transactions are kept.
    for _ in 0..3 {
        assert_eq!(
            mempool
                .next_transaction(&L2TxFilter::default())
                .unwrap()
                .0
                .initiator_account(),
            account0
        );
    }
    assert_eq!(
        mempool
            .next_transaction(&L2TxFilter::default())
            .unwrap()
            .0
            .initiator_account(),
        account1
    );
    assert!(!mempool.has_next(&L2TxFilter::default()));
}

#[test]
fn mempool_does_not_purge_all_accounts() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 1, None, None);
    let account0 = Address::random();
    let account1 = Address::random();
    let transactions = vec![
        gen_l2_tx(account0, Nonce(0)),
        gen_l2_tx(account0, Nonce(1)),
        gen_l2_tx(account1, Nonce(1)),
    ];
    mempool.insert_without_constraints(transactions, HashMap::new());
    // Mempool is full. Account 1 has tx with non-sequential nonce so it should be purged.
    // Txs from account 0 have sequential nonces but their number is greater than capacity; they should be kept.
    assert_eq!(mempool.get_mempool_info().purged_accounts, vec![account1]);
    // verify that good-to-go transactions are kept.
    for _ in 0..2 {
        assert_eq!(
            mempool
                .next_transaction(&L2TxFilter::default())
                .unwrap()
                .0
                .initiator_account(),
            account0
        );
    }
    assert!(!mempool.has_next(&L2TxFilter::default()));
}

#[test]
fn advance_after_block_removes_processed_txs_and_updates_nonce() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100, None, None);
    let account = Address::random();

    // Insert txs with nonces 0, 1, 2
    mempool.insert_without_constraints(
        vec![
            gen_l2_tx(account, Nonce(0)),
            gen_l2_tx(account, Nonce(1)),
            gen_l2_tx(account, Nonce(2)),
        ],
        HashMap::new(),
    );

    // Insert L1 transactions for priority_id = 0,1
    mempool.insert_without_constraints(
        vec![gen_l1_tx(PriorityOpId(0)), gen_l1_tx(PriorityOpId(1))],
        HashMap::new(),
    );

    // Advance to priority id 1 and account nonce 2.
    mempool.advance_after_block(AdvanceInput {
        next_priority_id: Some(PriorityOpId(1)),
        next_account_nonces: vec![(account, Nonce(2))],
    });

    // Check next_priority_id and l1_transactions are updated
    assert_eq!(mempool.next_priority_id(), PriorityOpId(1));
    assert!(!mempool.l1_transactions().contains_key(&PriorityOpId(0)));
    assert!(mempool.l1_transactions().contains_key(&PriorityOpId(1)));

    let tx = mempool.next_transaction(&L2TxFilter::default()).unwrap().0;
    let serial_id = match tx.common_data {
        ExecuteTransactionCommon::L1(tx) => tx.serial_id,
        _ => panic!("Expected L1 transaction"),
    };
    assert_eq!(serial_id, PriorityOpId(1));

    let tx = mempool.next_transaction(&L2TxFilter::default()).unwrap().0;
    assert_eq!(tx.nonce().unwrap(), Nonce(2));
    assert!(mempool.next_transaction(&L2TxFilter::default()).is_none());

    // l2_priority_queue should be empty after consuming the last tx
    assert!(mempool.l2_priority_queue().is_empty());
}

fn gen_l2_tx(address: Address, nonce: Nonce) -> Transaction {
    gen_l2_tx_with_timestamp(address, nonce, unix_timestamp_ms())
}

fn gen_l2_tx_with_timestamp(address: Address, nonce: Nonce, received_at_ms: u64) -> Transaction {
    let mut txn = L2Tx::new(
        Some(Address::default()),
        Vec::new(),
        nonce,
        Fee::default(),
        address,
        U256::zero(),
        vec![],
        Default::default(),
    );
    txn.received_timestamp_ms = received_at_ms;
    txn.into()
}

fn gen_l1_tx(priority_id: PriorityOpId) -> Transaction {
    let execute = Execute {
        contract_address: Some(Address::repeat_byte(0x11)),
        calldata: vec![1, 2, 3],
        factory_deps: vec![],
        value: U256::zero(),
    };
    let op_data = L1TxCommonData {
        sender: Address::random(),
        serial_id: priority_id,
        layer_2_tip_fee: U256::zero(),
        full_fee: U256::zero(),
        gas_limit: U256::zero(),
        max_fee_per_gas: U256::zero(),
        gas_per_pubdata_limit: U256::one(),
        op_processing_type: OpProcessingType::Common,
        priority_queue_type: PriorityQueueType::Deque,
        eth_block: 0,
        canonical_tx_hash: H256::zero(),
        to_mint: U256::zero(),
        refund_recipient: Address::random(),
    };

    Transaction {
        common_data: ExecuteTransactionCommon::L1(op_data),
        execute,
        received_timestamp_ms: 0,
        raw_bytes: None,
    }
}

fn view(transaction: Option<(Transaction, TransactionTimeRangeConstraint)>) -> (Address, u32) {
    let tx = transaction.unwrap().0;
    (tx.initiator_account(), tx.nonce().unwrap().0)
}

fn gen_transactions_for_filtering(input: Vec<(Address, Nonce, u64, u32)>) -> Vec<Transaction> {
    // Helper function to conveniently set `max_gas_per_pubdata_byte`.
    fn set_max_gas_per_pubdata_byte(tx: &mut Transaction, value: u32) {
        match &mut tx.common_data {
            ExecuteTransactionCommon::L2(data) => {
                data.fee.gas_per_pubdata_limit = U256::from(value)
            }
            _ => unreachable!(),
        };
    }
    input
        .into_iter()
        .map(|(account, nonce, tst, max_gas_per_pubdata)| {
            let mut tx = gen_l2_tx_with_timestamp(account, nonce, tst);
            set_max_gas_per_pubdata_byte(&mut tx, max_gas_per_pubdata);
            tx
        })
        .collect()
}
