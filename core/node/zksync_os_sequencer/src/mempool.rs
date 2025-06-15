// mempool.rs

use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use crossbeam_queue::SegQueue;
use futures_core::{FusedStream, Stream};
use futures_core::task::__internal::AtomicWaker;
use tokio::sync::Notify;
use vise::{Buckets, Counter, Histogram, LabeledFamily, Metrics};
use zk_os_forward_system::run::TxSource;
use zksync_types::l1::L1Tx;
use zksync_types::{Address, Execute, L1TxCommonData, PriorityOpId, Transaction, U256};
use crate::execution::metrics::PreimagesRocksDBMetrics;

/// Thread-safe FIFO mempool that can be polled as a `Stream`.
#[derive(Clone, Debug)]
pub struct Mempool {
    queue: Arc<SegQueue<Transaction>>,
    waker: Arc<AtomicWaker>,        // wakes one waiting consumer
}



#[derive(Debug, Metrics)]
#[metrics(prefix = "mempool")]
pub struct MempoolMetrics {
    #[metrics(labels = ["result"])]
    pub try_pop: LabeledFamily<&'static str, Counter<u64>>,
    #[metrics(buckets = Buckets::exponential(1.0..=1000000.0, 10.0))]
    pub len: Histogram<u64>,
}
#[vise::register]
pub(crate) static MEMPOOL_METRICS: vise::Global<MempoolMetrics> = vise::Global::new();


impl Mempool {
    pub fn new(forced_tx: Transaction) -> Self {
        let q = Arc::new(SegQueue::new());
        q.push(forced_tx);
        Self { queue: q, waker: Arc::new(AtomicWaker::new()) }
    }

    /* -------- producers ------------------------------------------- */

    pub fn insert(&self, tx: Transaction) {
        self.queue.push(tx);
        self.waker.wake();          // notify a waiting task
    }

    pub fn try_pop(&self) -> Option<Transaction> {
        let r = self.queue.pop();
        let metrics_key = if r.is_some() {
            "some"
        } else {
            "none"
        };
        MEMPOOL_METRICS.try_pop[&metrics_key].inc();
        MEMPOOL_METRICS.len.observe(self.queue.len() as u64);
        r
    }

    /* -------- consumer helpers ------------------------------------ */

    pub async fn next_tx(&self) -> Transaction {
        loop {
            if let Some(tx) = self.try_pop() {
                return tx;
            }
            futures_util::future::poll_fn(|cx| {
                self.waker.register(cx.waker());
                if let Some(tx) = self.try_pop() {
                    Poll::Ready(tx)
                } else {
                    Poll::Pending
                }
            })
                .await;
        }
    }
}

/* -------- Stream / FusedStream for &Mempool ---------------------- */

impl Stream for Mempool {
    type Item = Transaction;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // `self` is a pinned &mut Mempool; we can safely get a shared ref.
        let me: &Mempool = &*self;

        if let Some(tx) = me.try_pop() {
            return Poll::Ready(Some(tx));
        }

        me.waker.register(cx.waker());

        // race-check in case a tx arrived after register()
        if let Some(tx) = me.try_pop() {
            me.waker.take();
            Poll::Ready(Some(tx))
        } else {
            Poll::Pending
        }
    }
}

impl FusedStream for Mempool {
    fn is_terminated(&self) -> bool { false }     // endless stream
}

pub fn forced_deposit_transaction() -> Transaction{

    L1Tx {
        execute: Execute {
            contract_address: Some(Address::from_str("0x36615Cf349d7F6344891B1e7CA7C72883F5dc049").unwrap()),
            calldata: vec![],
            value: U256::from("100"),
            factory_deps: vec![],
        },
        common_data: L1TxCommonData {
            sender: Address::from_str("0x36615Cf349d7F6344891B1e7CA7C72883F5dc049").unwrap(),
            serial_id: PriorityOpId(1),
            layer_2_tip_fee: Default::default(),
            full_fee: U256::from("10000000000"),
            max_fee_per_gas: U256::from(1),
            gas_limit: U256::from("10000000000"),
            gas_per_pubdata_limit: U256::from(1000),
            op_processing_type: Default::default(),
            priority_queue_type: Default::default(),
            canonical_tx_hash: Default::default(),
            to_mint: U256::from("100000000000000000000000000000"),
            refund_recipient: Address::from_str("0x36615Cf349d7F6344891B1e7CA7C72883F5dc049").unwrap(),
            eth_block: 0,
        },
        received_timestamp_ms: 0,
    }.into()
}