use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use dashmap::DashMap;
use futures::future::BoxFuture;
use tokio::sync::{oneshot, watch};
use zksync_dal::Core;
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_types::L1BatchNumber;

use crate::{
    state_keeper::{updates::UpdatesManager, StateKeeperOutputHandler},
    vm_runner::VmRunnerIo,
};

#[async_trait]
pub trait OutputHandlerFactory: Debug + Send {
    async fn create_handler(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Box<dyn StateKeeperOutputHandler>>;
}

pub struct ConcurrentOutputHandlerFactory<Io: VmRunnerIo, F: OutputHandlerFactory> {
    pool: ConnectionPool<Core>,
    state: Arc<DashMap<L1BatchNumber, oneshot::Receiver<BoxFuture<'static, anyhow::Result<()>>>>>,
    io: Io,
    factory: F,
}

impl<Io: VmRunnerIo, F: OutputHandlerFactory> Debug for ConcurrentOutputHandlerFactory<Io, F> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConcurrentOutputHandlerFactory")
            .field("pool", &self.pool)
            .field("io", &self.io)
            .field("factory", &self.factory)
            .finish()
    }
}

impl<Io: VmRunnerIo + Clone, F: OutputHandlerFactory> ConcurrentOutputHandlerFactory<Io, F> {
    pub fn new(
        pool: ConnectionPool<Core>,
        io: Io,
        factory: F,
    ) -> (Self, ConcurrentOutputHandlerFactoryTask<Io>) {
        let state = Arc::new(DashMap::new());
        let task = ConcurrentOutputHandlerFactoryTask {
            pool: pool.clone(),
            io: io.clone(),
            state: state.clone(),
        };
        (
            Self {
                pool,
                state,
                io,
                factory,
            },
            task,
        )
    }
}

#[async_trait]
impl<Io: VmRunnerIo, F: OutputHandlerFactory> OutputHandlerFactory
    for ConcurrentOutputHandlerFactory<Io, F>
{
    async fn create_handler(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Box<dyn StateKeeperOutputHandler>> {
        let mut conn = self.pool.connection_tagged(Io::name()).await?;
        let latest_processed_batch = self.io.latest_processed_batch(&mut conn).await?;
        let last_processable_batch = self.io.last_ready_to_be_loaded_batch(&mut conn).await?;
        drop(conn);
        anyhow::ensure!(
            l1_batch_number > latest_processed_batch,
            "Cannot handle an already processed batch #{} (latest is #{})",
            l1_batch_number,
            latest_processed_batch
        );
        anyhow::ensure!(
            l1_batch_number <= last_processable_batch,
            "Cannot handle batch #{} as it is too far away from latest batch #{} (last processable batch is #{})",
            l1_batch_number,
            latest_processed_batch,
            last_processable_batch
        );

        let handler = self.factory.create_handler(l1_batch_number).await?;
        let (sender, receiver) = oneshot::channel();
        self.state.insert(l1_batch_number, receiver);
        Ok(Box::new(AsyncOutputHandler {
            internal: Some(OutputHandlerState::Running { handler, sender }),
        }))
    }
}

enum OutputHandlerState {
    Running {
        handler: Box<dyn StateKeeperOutputHandler>,
        sender: oneshot::Sender<BoxFuture<'static, anyhow::Result<()>>>,
    },
    Finished,
}

impl Debug for OutputHandlerState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutputHandlerState").finish()
    }
}

#[derive(Debug)]
struct AsyncOutputHandler {
    internal: Option<OutputHandlerState>,
}

#[async_trait]
impl StateKeeperOutputHandler for AsyncOutputHandler {
    async fn handle_l2_block(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        match &mut self.internal {
            Some(OutputHandlerState::Running { handler, .. }) => {
                handler.handle_l2_block(updates_manager).await
            }
            Some(OutputHandlerState::Finished) => {
                Err(anyhow::anyhow!("Cannot handle any more L2 blocks"))
            }
            None => Err(anyhow::anyhow!(
                "Unexpected state, missing output handler state"
            )),
        }
    }

    async fn handle_l1_batch(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        let state = self.internal.take();
        match state {
            Some(OutputHandlerState::Running {
                mut handler,
                sender,
            }) => {
                self.internal = Some(OutputHandlerState::Finished);
                sender
                    .send(Box::pin(async move {
                        handler.handle_l1_batch(updates_manager).await
                    }))
                    .ok();
                Ok(())
            }
            Some(OutputHandlerState::Finished) => {
                self.internal = state;
                Err(anyhow::anyhow!("Cannot handle any more L1 batches"))
            }
            None => Err(anyhow::anyhow!(
                "Unexpected state, missing output handler state"
            )),
        }
    }
}

pub struct ConcurrentOutputHandlerFactoryTask<Io: VmRunnerIo> {
    pool: ConnectionPool<Core>,
    io: Io,
    state: Arc<DashMap<L1BatchNumber, oneshot::Receiver<BoxFuture<'static, anyhow::Result<()>>>>>,
}

impl<Io: VmRunnerIo> ConcurrentOutputHandlerFactoryTask<Io> {
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        const SLEEP_INTERVAL: Duration = Duration::from_millis(50);

        let mut conn = self.pool.connection_tagged(Io::name()).await?;
        let mut latest_processed_batch = self.io.latest_processed_batch(&mut conn).await?;
        drop(conn);
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("`ConcurrentOutputHandlerFactoryTask` was interrupted");
                return Ok(());
            }
            match self.state.remove(&(latest_processed_batch + 1)) {
                None => {
                    tracing::debug!(
                        "Output handler for batch #{} has not been created yet",
                        latest_processed_batch + 1
                    );
                    tokio::time::sleep(SLEEP_INTERVAL).await;
                }
                Some((_, receiver)) => {
                    // Wait until the future is sent through the receiver, happens when
                    // `handle_l1_batch` is called on the corresponding output handler
                    let future = receiver.await?;
                    // Wait until the future is completed, meaning that the `handle_l1_batch`
                    // computation has finished, and we can consider this batch to be completed
                    future.await?;
                    latest_processed_batch += 1;
                    let mut conn = self.pool.connection_tagged(Io::name()).await?;
                    self.io
                        .mark_l1_batch_as_completed(&mut conn, latest_processed_batch)
                        .await?;
                    drop(conn);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::state_keeper::updates::UpdatesManager;
    use crate::state_keeper::StateKeeperOutputHandler;
    use crate::vm_runner::{ConcurrentOutputHandlerFactory, OutputHandlerFactory, VmRunnerIo};
    use async_trait::async_trait;
    use backon::{ConstantBuilder, Retryable};
    use multivm::interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::{watch, RwLock};
    use tokio::task::JoinHandle;
    use zksync_contracts::{BaseSystemContracts, SystemContractCode};
    use zksync_dal::Core;
    use zksync_db_connection::connection::Connection;
    use zksync_db_connection::connection_pool::ConnectionPool;
    use zksync_types::L1BatchNumber;

    #[derive(Debug, Default)]
    struct IoMock {
        current: L1BatchNumber,
        max: u32,
    }

    #[async_trait]
    impl VmRunnerIo for Arc<RwLock<IoMock>> {
        fn name() -> &'static str {
            "io_mock"
        }

        async fn latest_processed_batch(
            &self,
            _conn: &mut Connection<'_, Core>,
        ) -> anyhow::Result<L1BatchNumber> {
            Ok(self.read().await.current)
        }

        async fn last_ready_to_be_loaded_batch(
            &self,
            _conn: &mut Connection<'_, Core>,
        ) -> anyhow::Result<L1BatchNumber> {
            let io = self.read().await;
            Ok(io.current + io.max)
        }

        async fn mark_l1_batch_as_completed(
            &self,
            _conn: &mut Connection<'_, Core>,
            l1_batch_number: L1BatchNumber,
        ) -> anyhow::Result<()> {
            self.write().await.current = l1_batch_number;
            Ok(())
        }
    }

    #[derive(Debug)]
    struct TestOutputFactory {
        delays: HashMap<L1BatchNumber, Duration>,
    }

    #[async_trait]
    impl OutputHandlerFactory for TestOutputFactory {
        async fn create_handler(
            &mut self,
            l1_batch_number: L1BatchNumber,
        ) -> anyhow::Result<Box<dyn StateKeeperOutputHandler>> {
            let delay = self.delays.get(&l1_batch_number).copied();
            #[derive(Debug)]
            struct TestOutputHandler {
                delay: Option<Duration>,
            }
            #[async_trait]
            impl StateKeeperOutputHandler for TestOutputHandler {
                async fn handle_l2_block(
                    &mut self,
                    _updates_manager: &UpdatesManager,
                ) -> anyhow::Result<()> {
                    Ok(())
                }

                async fn handle_l1_batch(
                    &mut self,
                    _updates_manager: Arc<UpdatesManager>,
                ) -> anyhow::Result<()> {
                    if let Some(delay) = self.delay {
                        tokio::time::sleep(delay).await
                    }
                    Ok(())
                }
            }
            Ok(Box::new(TestOutputHandler { delay }))
        }
    }

    struct OutputHandlerTester {
        io: Arc<RwLock<IoMock>>,
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
            let (output_factory, task) =
                ConcurrentOutputHandlerFactory::new(pool, io.clone(), test_factory);
            let (stop_sender, stop_receiver) = watch::channel(false);
            let join_handle =
                tokio::task::spawn(async move { task.run(stop_receiver).await.unwrap() });
            let tasks = vec![join_handle];
            Self {
                io,
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

        async fn wait_for_batch(
            &self,
            l1_batch_number: L1BatchNumber,
            timeout: Duration,
        ) -> anyhow::Result<()> {
            const RETRY_INTERVAL: Duration = Duration::from_millis(500);

            let max_tries = (timeout.as_secs_f64() / RETRY_INTERVAL.as_secs_f64()).ceil() as u64;
            (|| async {
                let current = self.io.read().await.current;
                anyhow::ensure!(
                    current == l1_batch_number,
                    "Batch #{} has not been processed yet (current is #{})",
                    l1_batch_number,
                    current
                );
                Ok(())
            })
            .retry(
                &ConstantBuilder::default()
                    .with_delay(RETRY_INTERVAL)
                    .with_max_times(max_tries as usize),
            )
            .await
        }

        async fn wait_for_batch_progressively(
            &self,
            l1_batch_number: L1BatchNumber,
            timeout: Duration,
        ) -> anyhow::Result<()> {
            const SLEEP_INTERVAL: Duration = Duration::from_millis(500);

            let mut current = self.io.read().await.current;
            let max_tries = (timeout.as_secs_f64() / SLEEP_INTERVAL.as_secs_f64()).ceil() as u64;
            let mut try_num = 0;
            loop {
                tokio::time::sleep(SLEEP_INTERVAL).await;
                try_num += 1;
                if try_num >= max_tries {
                    anyhow::bail!("Timeout");
                }
                let new_current = self.io.read().await.current;
                // Ensure we did not go back in latest processed batch
                if new_current < current {
                    anyhow::bail!(
                        "Latest processed batch regressed to #{} back from #{}",
                        new_current,
                        current
                    );
                }
                current = new_current;
                if current >= l1_batch_number {
                    return Ok(());
                }
            }
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
            tester
                .wait_for_batch(i.into(), Duration::from_secs(10))
                .await?;
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
        tester
            .wait_for_batch_progressively(L1BatchNumber(9), Duration::from_secs(60))
            .await?;
        tester.stop_and_wait_for_all_tasks().await?;
        assert_eq!(io.read().await.current, L1BatchNumber(9));
        Ok(())
    }
}
