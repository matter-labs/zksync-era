//! Handling outputs produced by the state keeper.

use std::{fmt, sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_shared_resources::api::SyncState;
use zksync_types::{block::L2BlockHeader, L2BlockNumber};

use crate::{io::IoCursor, metrics::L1_BATCH_METRICS, updates::UpdatesManager};

/// Handler for state keeper outputs (L2 blocks and L1 batches).
#[async_trait]
pub trait StateKeeperOutputHandler: 'static + Send + Sync + fmt::Debug {
    /// Initializes this handler. This method will be called on state keeper initialization before any other calls.
    /// The default implementation does nothing.
    async fn initialize(&mut self, _cursor: &IoCursor) -> anyhow::Result<()> {
        Ok(())
    }

    /// Handles an L2 block produced by the state keeper.
    async fn handle_l2_block(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()>;

    /// Handles an L1 batch produced by the state keeper.
    async fn handle_l1_batch(
        &mut self,
        _updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// TODO
    async fn handle_l2_block_header(&mut self, _header: &L2BlockHeader) -> anyhow::Result<()> {
        Ok(())
    }

    /// TODO
    async fn rollback_pending_l2_block(
        &mut self,
        _l2_block_to_rollback: L2BlockNumber,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait]
impl StateKeeperOutputHandler for SyncState {
    async fn initialize(&mut self, cursor: &IoCursor) -> anyhow::Result<()> {
        let sealed_block_number = cursor.next_l2_block.saturating_sub(1);
        self.set_local_block(L2BlockNumber(sealed_block_number));
        Ok(())
    }

    async fn handle_l2_block(&mut self, _updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        Ok(())
    }

    async fn handle_l2_block_header(&mut self, header: &L2BlockHeader) -> anyhow::Result<()> {
        self.set_local_block(header.number);
        Ok(())
    }

    async fn handle_l1_batch(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        let sealed_block_number = updates_manager.l2_block.number;
        self.set_local_block(sealed_block_number);
        Ok(())
    }
}

/// Compound output handler plugged into the state keeper.
///
/// This handle aggregates one or more [`StateKeeperOutputHandler`]s executing their hooks
/// on each new L2 block / L1 batch produced by the state keeper. These are executed sequentially in the order
/// handlers were inserted into this `OutputHandler`. Errors from handlers are bubbled up to the state keeper level,
/// meaning that if a handler fails, the corresponding hook won't run for subsequent handlers.
#[derive(Debug)]
pub struct OutputHandler {
    inner: Vec<Box<dyn StateKeeperOutputHandler>>,
    last_l1_batch_sealed_at: Option<Instant>,
}

impl OutputHandler {
    /// Creates an output handler consisting of a single handler.
    pub fn new(main_handler: Box<dyn StateKeeperOutputHandler>) -> Self {
        Self {
            inner: vec![main_handler],
            last_l1_batch_sealed_at: None,
        }
    }

    /// Adds a new handler. Its hooks will be executed after all handlers inserted previously.
    #[must_use]
    pub fn with_handler(mut self, handler: Box<dyn StateKeeperOutputHandler>) -> Self {
        self.inner.push(handler);
        self
    }

    pub(crate) async fn initialize(&mut self, cursor: &IoCursor) -> anyhow::Result<()> {
        for handler in &mut self.inner {
            handler
                .initialize(cursor)
                .await
                .with_context(|| format!("failed initializing handler {handler:?}"))?;
        }
        Ok(())
    }

    #[tracing::instrument(
        name = "OutputHandler::handle_l2_block"
        skip_all,
        fields(l2_block = %updates_manager.l2_block.number)
    )]
    pub(crate) async fn handle_l2_block(
        &mut self,
        updates_manager: &UpdatesManager,
    ) -> anyhow::Result<()> {
        for handler in &mut self.inner {
            handler
                .handle_l2_block(updates_manager)
                .await
                .with_context(|| {
                    format!(
                        "failed handling L2 block {:?} on handler {handler:?}",
                        updates_manager.l2_block
                    )
                })?;
        }
        Ok(())
    }

    #[tracing::instrument(
        name = "OutputHandler::handle_l2_block_header"
        skip_all,
        fields(l2_block = %header.number)
    )]
    pub(crate) async fn handle_l2_block_header(
        &mut self,
        header: &L2BlockHeader,
    ) -> anyhow::Result<()> {
        for handler in &mut self.inner {
            handler
                .handle_l2_block_header(header)
                .await
                .with_context(|| {
                    format!(
                        "failed handling L2 block {:?} on handler {handler:?}",
                        header.number
                    )
                })?;
        }
        Ok(())
    }

    #[tracing::instrument(
        name = "OutputHandler::rollback_pending_l2_block"
        skip(self)
    )]
    pub(crate) async fn rollback_pending_l2_block(
        &mut self,
        l2_block_to_rollback: L2BlockNumber,
    ) -> anyhow::Result<()> {
        for handler in &mut self.inner {
            handler
                .rollback_pending_l2_block(l2_block_to_rollback)
                .await
                .with_context(|| {
                    format!("failed handling L2 block rollback {l2_block_to_rollback} on handler {handler:?}")
                })?;
        }
        Ok(())
    }

    #[tracing::instrument(
        name = "OutputHandler::handle_l1_batch"
        skip_all,
        fields(l1_batch = %updates_manager.l1_batch.number)
    )]
    pub(crate) async fn handle_l1_batch(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        for handler in &mut self.inner {
            handler
                .handle_l1_batch(updates_manager.clone())
                .await
                .with_context(|| {
                    format!(
                        "failed handling L1 batch #{} on handler {handler:?}",
                        updates_manager.l1_batch.number
                    )
                })?;
        }

        if let Some(last_l1_batch_sealed_at) = self.last_l1_batch_sealed_at {
            L1_BATCH_METRICS
                .seal_delta
                .observe(last_l1_batch_sealed_at.elapsed());
        }
        self.last_l1_batch_sealed_at = Some(Instant::now());

        Ok(())
    }
}
