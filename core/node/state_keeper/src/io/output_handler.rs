//! Handling outputs produced by the state keeper.

use std::{fmt, sync::Arc};

use anyhow::Context as _;
use async_trait::async_trait;

use crate::{io::IoCursor, updates::UpdatesManager};

/// Handler for state keeper outputs (L2 blocks and L1 batches).
#[async_trait]
pub trait StateKeeperOutputHandler: 'static + Send + fmt::Debug {
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
        Ok(())
    }
}
