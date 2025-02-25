//! Handling outputs produced by the state keeper.

use std::{fmt, sync::Arc};

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_types::block::UnsealedL1BatchHeader;

use crate::{io::IoCursor, updates::UpdatesManager};

/// Handler for state keeper outputs (L2 blocks and L1 batches).
#[async_trait]
pub trait StateKeeperOutputHandler: 'static + Send + fmt::Debug {
    /// Initializes this handler. This method will be called on state keeper initialization before any other calls.
    /// The default implementation does nothing.
    async fn initialize(&mut self, _cursor: &IoCursor) -> anyhow::Result<()> {
        Ok(())
    }

    /// Handles a new L1 batch opening.
    async fn handle_open_batch(&mut self, _header: UnsealedL1BatchHeader) -> anyhow::Result<()> {
        Ok(())
    }

    /// Handles an L1 batch produced by the state keeper.
    async fn handle_block(&mut self, updates_manager: Arc<UpdatesManager>) -> anyhow::Result<()>;
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
}

impl OutputHandler {
    /// Creates an output handler consisting of a single handler.
    pub fn new(main_handler: Box<dyn StateKeeperOutputHandler>) -> Self {
        Self {
            inner: vec![main_handler],
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
        name = "OutputHandler::handle_open_batch"
        skip_all,
        fields(l1_batch = %header.number)
    )]
    pub(crate) async fn handle_open_batch(
        &mut self,
        header: UnsealedL1BatchHeader,
    ) -> anyhow::Result<()> {
        for handler in &mut self.inner {
            handler
                .handle_open_batch(header.clone())
                .await
                .with_context(|| {
                    format!(
                        "failed handling opened batch {:?} on handler {handler:?}",
                        header.number
                    )
                })?;
        }
        Ok(())
    }

    #[tracing::instrument(
        name = "OutputHandler::handle_l2_block"
        skip_all,
        fields(l2_block = %updates_manager.l2_block.number)
    )]
    pub(crate) async fn handle_block(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        for handler in &mut self.inner {
            handler
                .handle_block(updates_manager.clone())
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
}
