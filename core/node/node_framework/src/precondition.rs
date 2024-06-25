use std::{fmt, sync::Arc};

use tokio::sync::Barrier;

use crate::{service::StopReceiver, task::TaskId};

#[async_trait::async_trait]
pub trait Precondition: 'static + Send + Sync {
    /// Unique name of the precondition.
    fn id(&self) -> TaskId;

    async fn check(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()>;
}

impl dyn Precondition {
    /// An internal helper method that runs a precondition check and lifts the barrier as soon
    /// as the check is finished.
    pub(super) async fn check_with_barrier(
        self: Box<Self>,
        mut stop_receiver: StopReceiver,
        preconditions_barrier: Arc<Barrier>,
    ) -> anyhow::Result<()> {
        self.check(stop_receiver.clone()).await?;
        tokio::select! {
            _ = preconditions_barrier.wait() => {
                Ok(())
            }
            _ = stop_receiver.0.changed() => {
                Ok(())
            }
        }
    }
}

impl fmt::Debug for dyn Precondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Precondition")
            .field("name", &self.id())
            .finish()
    }
}
