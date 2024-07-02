use std::{
    fmt::{Debug, Formatter},
    mem,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use async_trait::async_trait;
use dashmap::DashMap;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};
use zksync_dal::{ConnectionPool, Core};
use zksync_state_keeper::{StateKeeperOutputHandler, UpdatesManager};
use zksync_types::L1BatchNumber;

use crate::{metrics::METRICS, VmRunnerIo};

type BatchReceiver = oneshot::Receiver<JoinHandle<anyhow::Result<Instant>>>;

/// Functionality to produce a [`StateKeeperOutputHandler`] implementation for a specific L1 batch.
///
/// The idea behind this trait is that often handling output data is independent of the order of the
/// batch that data belongs to. In other words, one could be handling output of batch #100 and #1000
/// simultaneously. Implementing this trait signifies that this property is held for the data the
/// implementation is responsible for.
#[async_trait]
pub trait OutputHandlerFactory: Debug + Send {
    /// Creates a [`StateKeeperOutputHandler`] implementation for the provided L1 batch. Only
    /// supposed to be used for the L1 batch data it was created against. Using it for anything else
    /// will lead to errors.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    async fn create_handler(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Box<dyn StateKeeperOutputHandler>>;
}

/// A delegator factory that requires an underlying factory `F` that does the actual work, however
/// this struct is orchestrated such that any output handler it produces has a non-blocking
/// `handle_l1_batch` implementation (where the heaviest work is expected to happen).
///
/// Once the asynchronous work done in `handle_l1_batch` finishes it is also guaranteed to mark the
/// batch is processed by `Io`. It is guaranteed, however, that for any processed batch all batches
/// preceding it are also processed. No guarantees about subsequent batches. For example, if
/// batches #1, #2, #3, #5, #9, #100 are processed then only batches #{1-3} will be marked as
/// processed and #3 would be the latest processed batch as defined in [`VmRunnerIo`].
pub struct ConcurrentOutputHandlerFactory<Io: VmRunnerIo, F: OutputHandlerFactory> {
    pool: ConnectionPool<Core>,
    state: Arc<DashMap<L1BatchNumber, BatchReceiver>>,
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
    /// Creates a new concurrent delegator factory using provided Postgres pool, VM runner IO
    /// and underlying output handler factory.
    ///
    /// Returns a [`ConcurrentOutputHandlerFactoryTask`] which is supposed to be run by the caller.
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
        let mut conn = self.pool.connection_tagged(self.io.name()).await?;
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
        Ok(Box::new(AsyncOutputHandler::Running { handler, sender }))
    }
}

enum AsyncOutputHandler {
    Running {
        handler: Box<dyn StateKeeperOutputHandler>,
        sender: oneshot::Sender<JoinHandle<anyhow::Result<Instant>>>,
    },
    Finished,
}

impl Debug for AsyncOutputHandler {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AsyncOutputHandler::Running { handler, .. } => f
                .debug_struct("AsyncOutputHandler::Running")
                .field("handler", handler)
                .finish(),
            AsyncOutputHandler::Finished => f.debug_struct("AsyncOutputHandler::Finished").finish(),
        }
    }
}

#[async_trait]
impl StateKeeperOutputHandler for AsyncOutputHandler {
    async fn handle_l2_block(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        match self {
            AsyncOutputHandler::Running { handler, .. } => {
                handler.handle_l2_block(updates_manager).await
            }
            AsyncOutputHandler::Finished => {
                Err(anyhow::anyhow!("Cannot handle any more L2 blocks"))
            }
        }
    }

    async fn handle_l1_batch(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        let state = mem::replace(self, AsyncOutputHandler::Finished);
        match state {
            AsyncOutputHandler::Running {
                mut handler,
                sender,
            } => {
                sender
                    .send(tokio::task::spawn(async move {
                        let started_at = Instant::now();
                        let result = handler.handle_l1_batch(updates_manager).await;
                        METRICS.output_handle_time.observe(started_at.elapsed());
                        result.map(|_| started_at)
                    }))
                    .ok();
                Ok(())
            }
            AsyncOutputHandler::Finished => {
                Err(anyhow::anyhow!("Cannot handle any more L1 batches"))
            }
        }
    }
}

/// A runnable task that continually awaits for the very next unprocessed batch to be processed and
/// marks it as so using `Io`.
pub struct ConcurrentOutputHandlerFactoryTask<Io: VmRunnerIo> {
    pool: ConnectionPool<Core>,
    io: Io,
    state: Arc<DashMap<L1BatchNumber, BatchReceiver>>,
}

impl<Io: VmRunnerIo> Debug for ConcurrentOutputHandlerFactoryTask<Io> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConcurrentOutputHandlerFactoryTask")
            .field("pool", &self.pool)
            .field("io", &self.io)
            .finish()
    }
}

impl<Io: VmRunnerIo> ConcurrentOutputHandlerFactoryTask<Io> {
    /// Access the underlying [`VmRunnerIo`].
    pub fn io(&self) -> &Io {
        &self.io
    }

    /// Starts running the task which is supposed to last until the end of the node's lifetime.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        const SLEEP_INTERVAL: Duration = Duration::from_millis(50);

        let mut conn = self.pool.connection_tagged(self.io.name()).await?;
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
                    // Wait until the `JoinHandle` is sent through the receiver, happens when
                    // `handle_l1_batch` is called on the corresponding output handler
                    let handle = receiver
                        .await
                        .context("handler was dropped before the batch was fully processed")?;
                    // Wait until the handle is resolved, meaning that the `handle_l1_batch`
                    // computation has finished, and we can consider this batch to be completed
                    let started_at = handle
                        .await
                        .context("failed to await for batch to be processed")??;
                    latest_processed_batch += 1;
                    let mut conn = self.pool.connection_tagged(self.io.name()).await?;
                    self.io
                        .mark_l1_batch_as_completed(&mut conn, latest_processed_batch, started_at)
                        .await?;
                    METRICS
                        .last_processed_batch
                        .set(latest_processed_batch.0.into());
                }
            }
        }
    }
}
