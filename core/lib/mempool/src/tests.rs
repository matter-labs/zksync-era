use crate::{mempool_store::MempoolStore, types::L2TxFilter};
use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;
use zksync_types::fee::Fee;
use zksync_types::helpers::unix_timestamp_ms;
use zksync_types::l1::{OpProcessingType, PriorityQueueType};
use zksync_types::l2::L2Tx;
use zksync_types::{Address, ExecuteTransactionCommon, L1TxCommonData, PriorityOpId, H256, U256};
use zksync_types::{Execute, Nonce, Transaction};

#[test]
fn basic_flow() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100);
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
    mempool.insert(transactions, HashMap::new());
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
    // unclog second account and insert more txns
    mempool.insert(
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
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100);
    let account = Address::random();
    let transactions = vec![
        gen_l2_tx(account, Nonce(6)),
        gen_l2_tx(account, Nonce(7)),
        gen_l2_tx(account, Nonce(9)),
    ];
    let mut nonces = HashMap::new();
    nonces.insert(account, Nonce(5));
    mempool.insert(transactions, nonces);
    assert_eq!(mempool.next_transaction(&L2TxFilter::default()), None);
    // missing transaction unclogs mempool
    mempool.insert(vec![gen_l2_tx(account, Nonce(5))], HashMap::new());
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
    mempool.insert(vec![gen_l2_tx(account, Nonce(8))], HashMap::new());
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
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100);
    let account = Address::random();
    let transactions = vec![
        gen_l2_tx(account, Nonce(0)),
        gen_l2_tx(account, Nonce(1)),
        gen_l1_tx(PriorityOpId(0)),
    ];
    mempool.insert(transactions, HashMap::new());
    assert!(mempool
        .next_transaction(&L2TxFilter::default())
        .unwrap()
        .is_l1())
}

#[test]
fn l1_txns_priority_id() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100);
    let transactions = vec![
        gen_l1_tx(PriorityOpId(1)),
        gen_l1_tx(PriorityOpId(2)),
        gen_l1_tx(PriorityOpId(3)),
    ];
    mempool.insert(transactions, HashMap::new());
    assert!(mempool.next_transaction(&L2TxFilter::default()).is_none());
    mempool.insert(vec![gen_l1_tx(PriorityOpId(0))], HashMap::new());
    for idx in 0..4 {
        let data = mempool
            .next_transaction(&L2TxFilter::default())
            .unwrap()
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
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100);
    let account = Address::random();
    let transactions = vec![
        gen_l2_tx(account, Nonce(0)),
        gen_l2_tx(account, Nonce(1)),
        gen_l2_tx(account, Nonce(2)),
        gen_l2_tx(account, Nonce(3)),
        gen_l2_tx(account, Nonce(5)),
    ];
    mempool.insert(transactions, HashMap::new());
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
    mempool.insert(vec![gen_l2_tx(account, Nonce(1))], HashMap::new());
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
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100);
    let account = Address::random();
    mempool.insert(vec![gen_l2_tx(account, Nonce(0))], HashMap::new());
    // replace it
    mempool.insert(
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
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100);
    let account0 = Address::random();
    let account1 = Address::random();
    let transactions = vec![gen_l2_tx(account0, Nonce(0)), gen_l2_tx(account1, Nonce(0))];
    mempool.insert(transactions, HashMap::new());
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
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100);
    let account0 = Address::random();
    let account1 = Address::random();
    let transactions = vec![
        gen_l2_tx(account0, Nonce(0)),
        gen_l2_tx(account0, Nonce(1)),
        gen_l2_tx(account0, Nonce(2)),
        gen_l2_tx(account0, Nonce(3)),
        gen_l2_tx(account1, Nonce(1)),
    ];
    mempool.insert(transactions, HashMap::new());
    assert_eq!(mempool.stats().l2_transaction_count, 5);
    // replacement
    mempool.insert(vec![gen_l2_tx(account0, Nonce(2))], HashMap::new());
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
        l1_gas_price: 0u64,
        fee_per_gas: 0u64,
        gas_per_pubdata: 1u32,
    };
    // No-op filter that fetches any transaction.
    let filter_zero = L2TxFilter {
        l1_gas_price: 0u64,
        fee_per_gas: 0u64,
        gas_per_pubdata: 0u32,
    };

    let mut mempool = MempoolStore::new(PriorityOpId(0), 100);
    let account0 = Address::random();
    let account1 = Address::random();

    // First account will have two transactions: one with too low pubdata price and one with the right value.
    // Second account will have just one transaction with the right value.
    mempool.insert(
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
        l1_gas_price: 0u64,
        fee_per_gas: 0u64,
        gas_per_pubdata: 1u32,
    };
    // No-op filter that fetches any transaction.
    let filter_zero = L2TxFilter {
        l1_gas_price: 0u64,
        fee_per_gas: 0u64,
        gas_per_pubdata: 0u32,
    };
    let mut mempool = MempoolStore::new(PriorityOpId(0), 100);
    let account0 = Address::random();
    let account1 = Address::random();

    mempool.insert(
        gen_transactions_for_filtering(vec![
            (account0, Nonce(0), unix_timestamp_ms(), 0),
            (account0, Nonce(1), unix_timestamp_ms(), 1),
            (account1, Nonce(0), unix_timestamp_ms() + 10, 1),
        ]),
        HashMap::new(),
    );
    assert!(mempool.get_mempool_info().stashed_accounts.is_empty());
    assert_eq!(
        view(mempool.next_transaction(&filter_non_zero)),
        (account1, 0)
    );
    assert_eq!(mempool.get_mempool_info().stashed_accounts, vec![account0]);
    assert!(mempool.next_transaction(&filter_zero).is_none());
}

#[test]
fn mempool_capacity() {
    let mut mempool = MempoolStore::new(PriorityOpId(0), 5);
    let account0 = Address::random();
    let account1 = Address::random();
    let account2 = Address::random();
    let transactions = vec![
        gen_l2_tx(account0, Nonce(0)),
        gen_l2_tx(account0, Nonce(1)),
        gen_l2_tx(account0, Nonce(2)),
        gen_l2_tx(account1, Nonce(1)),
        gen_l2_tx(account2, Nonce(1)),
    ];
    mempool.insert(transactions, HashMap::new());
    // the mempool is full. Accounts with non-sequential nonces got stashed
    assert_eq!(
        HashSet::<_>::from_iter(mempool.get_mempool_info().purged_accounts),
        HashSet::<_>::from_iter(vec![account1, account2]),
    );
    // verify that existing good-to-go transactions and new ones got picked
    mempool.insert(
        vec![gen_l2_tx_with_timestamp(
            account1,
            Nonce(0),
            unix_timestamp_ms() + 1,
        )],
        HashMap::new(),
    );
    for _ in 0..3 {
        assert_eq!(
            mempool
                .next_transaction(&L2TxFilter::default())
                .unwrap()
                .initiator_account(),
            account0
        );
    }
    assert_eq!(
        mempool
            .next_transaction(&L2TxFilter::default())
            .unwrap()
            .initiator_account(),
        account1
    );
}

fn gen_l2_tx(address: Address, nonce: Nonce) -> Transaction {
    gen_l2_tx_with_timestamp(address, nonce, unix_timestamp_ms())
}

fn gen_l2_tx_with_timestamp(address: Address, nonce: Nonce, received_at_ms: u64) -> Transaction {
    let mut txn = L2Tx::new(
        Address::default(),
        Vec::new(),
        nonce,
        Fee::default(),
        address,
        U256::zero(),
        None,
        Default::default(),
    );
    txn.received_timestamp_ms = received_at_ms;
    txn.into()
}

fn gen_l1_tx(priority_id: PriorityOpId) -> Transaction {
    let execute = Execute {
        contract_address: Address::repeat_byte(0x11),
        calldata: vec![1, 2, 3],
        factory_deps: None,
        value: U256::zero(),
    };
    let op_data = L1TxCommonData {
        sender: Address::random(),
        serial_id: priority_id,
        deadline_block: 100000,
        layer_2_tip_fee: U256::zero(),
        full_fee: U256::zero(),
        gas_limit: U256::zero(),
        max_fee_per_gas: U256::zero(),
        gas_per_pubdata_limit: U256::one(),
        op_processing_type: OpProcessingType::Common,
        priority_queue_type: PriorityQueueType::Deque,
        eth_hash: H256::zero(),
        eth_block: 1,
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

fn view(transaction: Option<Transaction>) -> (Address, u32) {
    let tx = transaction.unwrap();
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
