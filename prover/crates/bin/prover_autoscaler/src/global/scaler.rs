use super::{queuer, watcher};
use crate::{cluster_types::Clusters, task_wiring::Task};

pub struct Scaler {
    watcher: watcher::Watcher,
    queuer: queuer::Queuer,
}

impl Scaler {
    pub fn new(watcher: watcher::Watcher, queuer: queuer::Queuer) -> Self {
        Self { watcher, queuer }
    }

    fn run(&self, q: queuer::Queue, clusters: &Clusters) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for Scaler {
    async fn invoke(&self) -> anyhow::Result<()> {
        let q = self.queuer.get_queue().await.unwrap();
        let clusters = self.watcher.clusters.lock().await;
        let _ = self.run(q, &*clusters);
        Ok(())
    }
}
