use std::{collections::HashMap, sync::Arc, time::Duration};

use multivm::interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode};
use tokio::{
    sync::{watch, RwLock},
    task::JoinHandle,
};
use zksync_contracts::{BaseSystemContracts, SystemContractCode};
use zksync_dal::{ConnectionPool, Core};
use zksync_state_keeper::UpdatesManager;
use zksync_types::L1BatchNumber;

use crate::{
    tests::{wait, IoMock, TestOutputFactory},
    ConcurrentOutputHandlerFactory, OutputHandlerFactory,
};

struct OutputHandlerTester {
    output_factory: ConcurrentOutputHandlerFactory<Arc<RwLock<IoMock>>, TestOutputFactory>,
    tasks: Vec<JoinHandle<()>>,
    stop_sender: watch::Sender<bool>,
}

impl OutputHandlerTester {
    fn new(
        io: Arc<RwLock<IoMock>>,
        pool: ConnectionPool<Core>,
        delays: HashMap<L1BatchNumber, Duration>,
    ) -> Self {
        let test_factory = TestOutputFactory { delays };
        let (output_factory, task) = ConcurrentOutputHandlerFactory::new(pool, io, test_factory);
        let (stop_sender, stop_receiver) = watch::channel(false);
        let join_handle = tokio::task::spawn(async move { task.run(stop_receiver).await.unwrap() });
        let tasks = vec![join_handle];
        Self {
            output_factory,
            tasks,
            stop_sender,
        }
    }

    async fn spawn_test_task(&mut self, l1_batch_number: L1BatchNumber) -> anyhow::Result<()> {
        let mut output_handler = self.output_factory.create_handler(l1_batch_number).await?;
        let join_handle = tokio::task::spawn(async move {
            let l1_batch_env = L1BatchEnv {
                previous_batch_hash: None,
                number: Default::default(),
                timestamp: 0,
                fee_input: Default::default(),
                fee_account: Default::default(),
                enforced_base_fee: None,
                first_l2_block: L2BlockEnv {
                    number: 0,
                    timestamp: 0,
                    prev_block_hash: Default::default(),
                    max_virtual_blocks_to_create: 0,
                },
            };
            let system_env = SystemEnv {
                zk_porter_available: false,
                version: Default::default(),
                base_system_smart_contracts: BaseSystemContracts {
                    bootloader: SystemContractCode {
                        code: vec![],
                        hash: Default::default(),
                    },
                    default_aa: SystemContractCode {
                        code: vec![],
                        hash: Default::default(),
                    },
                },
                bootloader_gas_limit: 0,
                execution_mode: TxExecutionMode::VerifyExecute,
                default_validation_computational_gas_limit: 0,
                chain_id: Default::default(),
            };
            let updates_manager = UpdatesManager::new(&l1_batch_env, &system_env);
            output_handler
                .handle_l2_block(&updates_manager)
                .await
                .unwrap();
            output_handler
                .handle_l1_batch(Arc::new(updates_manager))
                .await
                .unwrap();
        });
        self.tasks.push(join_handle);
        Ok(())
    }

    async fn stop_and_wait_for_all_tasks(self) -> anyhow::Result<()> {
        self.stop_sender.send(true)?;
        futures::future::join_all(self.tasks).await;
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn monotonically_progress_processed_batches() -> anyhow::Result<()> {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let io = Arc::new(RwLock::new(IoMock {
        current: 0.into(),
        max: 10,
    }));
    // Distribute progressively higher delays for higher batches so that we can observe
    // each batch being marked as processed. In other words, batch 1 would be marked as processed,
    // then there will be a minimum 1 sec of delay (more in <10 thread environments), then batch
    // 2 would be marked as processed etc.
    let delays = (1..10)
        .map(|i| (L1BatchNumber(i), Duration::from_secs(i as u64)))
        .collect();
    let mut tester = OutputHandlerTester::new(io.clone(), pool, delays);
    for i in 1..10 {
        tester.spawn_test_task(i.into()).await?;
    }
    assert_eq!(io.read().await.current, L1BatchNumber(0));
    for i in 1..10 {
        wait::for_batch(io.clone(), i.into(), Duration::from_secs(10)).await?;
    }
    tester.stop_and_wait_for_all_tasks().await?;
    assert_eq!(io.read().await.current, L1BatchNumber(9));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn do_not_progress_with_gaps() -> anyhow::Result<()> {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let io = Arc::new(RwLock::new(IoMock {
        current: 0.into(),
        max: 10,
    }));
    // Distribute progressively lower delays for higher batches so that we can observe last
    // processed batch not move until the first batch (with longest delay) is processed.
    let delays = (1..10)
        .map(|i| (L1BatchNumber(i), Duration::from_secs(10 - i as u64)))
        .collect();
    let mut tester = OutputHandlerTester::new(io.clone(), pool, delays);
    for i in 1..10 {
        tester.spawn_test_task(i.into()).await?;
    }
    assert_eq!(io.read().await.current, L1BatchNumber(0));
    wait::for_batch_progressively(io.clone(), L1BatchNumber(9), Duration::from_secs(60)).await?;
    tester.stop_and_wait_for_all_tasks().await?;
    assert_eq!(io.read().await.current, L1BatchNumber(9));
    Ok(())
}
