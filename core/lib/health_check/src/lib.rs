use futures::{future, FutureExt};
use serde::Serialize;
use tokio::sync::watch;

use std::{collections::HashMap, thread};

/// Public re-export for other crates to be able to implement the interface.
pub use async_trait::async_trait;

/// Health status returned as a part of `Health`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HealthStatus {
    /// Component is initializing and is not ready yet.
    NotReady,
    /// Component is ready for operations.
    Ready,
    /// Component is shut down.
    ShutDown,
    /// Component has been abnormally interrupted by a panic.
    Panicked,
}

impl HealthStatus {
    /// Checks whether a component is ready according to this status.
    pub fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }

    fn priority_for_aggregation(self) -> usize {
        match self {
            Self::Ready => 0,
            Self::ShutDown => 1,
            Self::NotReady => 2,
            Self::Panicked => 3,
        }
    }
}

/// Health of a single component.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Health {
    status: HealthStatus,
    /// Component-specific details allowing to assess whether the component is healthy or not.
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

impl Health {
    /// Sets health details.
    #[must_use]
    pub fn with_details<T: Serialize>(mut self, details: T) -> Self {
        let details = serde_json::to_value(details).expect("Failed serializing `Health` details");
        self.details = Some(details);
        self
    }

    /// Returns the overall health status.
    pub fn status(&self) -> HealthStatus {
        self.status
    }
}

impl From<HealthStatus> for Health {
    fn from(status: HealthStatus) -> Self {
        Self {
            status,
            details: None,
        }
    }
}

/// Health information for an application consisting of multiple components.
#[derive(Debug, Serialize)]
pub struct AppHealth {
    #[serde(flatten)]
    inner: Health,
    components: HashMap<&'static str, Health>,
}

impl AppHealth {
    /// Aggregates health info from the provided checks.
    pub async fn new(health_checks: &[Box<dyn CheckHealth>]) -> Self {
        let check_futures = health_checks.iter().map(|check| {
            let check_name = check.name();
            check.check_health().map(move |health| (check_name, health))
        });
        let components: HashMap<_, _> = future::join_all(check_futures).await.into_iter().collect();

        let aggregated_status = components
            .values()
            .map(|health| health.status)
            .max_by_key(|status| status.priority_for_aggregation())
            .unwrap_or(HealthStatus::Ready);
        let inner = aggregated_status.into();

        Self { inner, components }
    }

    pub fn is_ready(&self) -> bool {
        self.inner.status.is_ready()
    }
}

/// Interface to be used for health checks.
#[async_trait]
pub trait CheckHealth: Send + Sync + 'static {
    /// Unique name of the component.
    fn name(&self) -> &'static str;
    /// Checks health of the component.
    async fn check_health(&self) -> Health;
}

/// Basic implementation of [`CheckHealth`] trait that can be updated using a matching [`HealthUpdater`].
#[derive(Debug)]
pub struct ReactiveHealthCheck {
    name: &'static str,
    health_receiver: watch::Receiver<Health>,
}

impl ReactiveHealthCheck {
    /// Creates a health check together with an updater that can be used to update it.
    /// The check will return [`HealthStatus::NotReady`] initially.
    pub fn new(name: &'static str) -> (Self, HealthUpdater) {
        let (health_sender, health_receiver) = watch::channel(HealthStatus::NotReady.into());
        let this = Self {
            name,
            health_receiver,
        };
        let updater = HealthUpdater {
            name,
            health_sender,
        };
        (this, updater)
    }
}

#[async_trait]
impl CheckHealth for ReactiveHealthCheck {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn check_health(&self) -> Health {
        self.health_receiver.borrow().clone()
    }
}

/// Updater for [`ReactiveHealthCheck`]. Can be created using [`ReactiveHealthCheck::new()`].
///
/// On drop, will automatically update status to [`HealthStatus::ShutDown`], or to [`HealthStatus::Panicked`]
/// if the dropping thread is panicking.
#[derive(Debug)]
pub struct HealthUpdater {
    name: &'static str,
    health_sender: watch::Sender<Health>,
}

impl HealthUpdater {
    /// Updates the health check information, returning if a change occurred from previous state.
    /// Note, description change on Health is counted as a change, even if status is the same.
    /// I.E. `Health { Ready, None }` to `Health { Ready, Some(_) }` is considered a change.
    pub fn update(&self, health: Health) -> bool {
        let old_health = self.health_sender.send_replace(health.clone());
        if old_health != health {
            tracing::debug!("changed health from {:?} to {:?}", old_health, health);
            return true;
        }
        false
    }

    /// Creates a [`ReactiveHealthCheck`] attached to this updater. This allows not retaining the initial health check
    /// returned by [`ReactiveHealthCheck::new()`].
    pub fn subscribe(&self) -> ReactiveHealthCheck {
        ReactiveHealthCheck {
            name: self.name,
            health_receiver: self.health_sender.subscribe(),
        }
    }
}

impl Drop for HealthUpdater {
    fn drop(&mut self) {
        let terminal_health = if thread::panicking() {
            HealthStatus::Panicked.into()
        } else {
            HealthStatus::ShutDown.into()
        };
        self.health_sender.send_replace(terminal_health);
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[tokio::test]
    async fn updating_health_status() {
        let (health_check, health_updater) = ReactiveHealthCheck::new("test");
        assert_eq!(health_check.name(), "test");
        assert_matches!(
            health_check.check_health().await.status(),
            HealthStatus::NotReady
        );

        health_updater.update(HealthStatus::Ready.into());
        assert_matches!(
            health_check.check_health().await.status(),
            HealthStatus::Ready
        );

        drop(health_updater);
        assert_matches!(
            health_check.check_health().await.status(),
            HealthStatus::ShutDown
        );
    }

    #[tokio::test]
    async fn updating_health_status_after_panic() {
        let (health_check, health_updater) = ReactiveHealthCheck::new("test");
        let task = tokio::spawn(async move {
            health_updater.update(HealthStatus::Ready.into());
            panic!("oops");
        });
        assert!(task.await.unwrap_err().is_panic());

        assert_matches!(
            health_check.check_health().await.status(),
            HealthStatus::Panicked
        );
    }

    #[tokio::test]
    async fn updating_health_status_return_value() {
        let (health_check, health_updater) = ReactiveHealthCheck::new("test");
        assert_matches!(
            health_check.check_health().await.status(),
            HealthStatus::NotReady
        );

        let updated = health_updater.update(HealthStatus::Ready.into());
        assert!(updated);
        assert_matches!(
            health_check.check_health().await.status(),
            HealthStatus::Ready
        );

        let updated = health_updater.update(HealthStatus::Ready.into());
        assert!(!updated);

        let health: Health = HealthStatus::Ready.into();
        let health = health.with_details("new details are treated as status change");
        let updated = health_updater.update(health);
        assert!(updated);
    }
}
