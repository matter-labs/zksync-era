use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

// Public re-export for other crates to be able to implement the interface.
pub use async_trait::async_trait;
use futures::future;
use serde::Serialize;
use tokio::sync::watch;

/// Health status returned as a part of `Health`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HealthStatus {
    /// Component is initializing and is not ready yet.
    NotReady,
    /// Component is ready for operations.
    Ready,
    /// Component is affected by some non-fatal issue. The component is still considered healthy.
    Affected,
    /// Component is shut down.
    ShutDown,
    /// Component has been abnormally interrupted by a panic.
    Panicked,
}

impl HealthStatus {
    /// Checks whether a component is healthy according to this status.
    pub fn is_healthy(self) -> bool {
        matches!(self, Self::Ready | Self::Affected)
    }

    fn priority_for_aggregation(self) -> usize {
        match self {
            Self::Ready => 0,
            Self::Affected => 1,
            Self::ShutDown => 2,
            Self::NotReady => 3,
            Self::Panicked => 4,
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

/// Application health check aggregating health from multiple components.
#[derive(Debug, Default)]
pub struct AppHealthCheck(Mutex<Vec<Arc<dyn CheckHealth>>>);

impl AppHealthCheck {
    /// Inserts health check for a component.
    pub fn insert_component(&self, health_check: ReactiveHealthCheck) {
        self.insert_custom_component(Arc::new(health_check));
    }

    /// Inserts a custom health check for a component.
    pub fn insert_custom_component(&self, health_check: Arc<dyn CheckHealth>) {
        let health_check_name = health_check.name();
        let mut guard = self.0.lock().expect("`AppHealthCheck` is poisoned");
        if guard.iter().any(|check| check.name() == health_check_name) {
            tracing::warn!(
                "Health check with name `{health_check_name}` is redefined; only the last mention \
                 will be present in `/health` endpoint output"
            );
        }
        guard.push(health_check);
    }

    /// Checks the overall application health. This will query all component checks concurrently.
    pub async fn check_health(&self) -> AppHealth {
        // Clone checks so that we don't hold a lock for them across a wait point.
        let health_checks = self.0.lock().expect("`AppHealthCheck` is poisoned").clone();
        AppHealth::new(&health_checks).await
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
    async fn new(health_checks: &[Arc<dyn CheckHealth>]) -> Self {
        let check_futures = health_checks
            .iter()
            .map(|check| Self::check_health_with_time_limit(check.as_ref()));
        let components: HashMap<_, _> = future::join_all(check_futures).await.into_iter().collect();

        let aggregated_status = components
            .values()
            .map(|health| health.status)
            .max_by_key(|status| status.priority_for_aggregation())
            .unwrap_or(HealthStatus::Ready);
        let inner = aggregated_status.into();

        let this = Self { inner, components };
        if !this.inner.status.is_healthy() {
            // Only log non-ready application health so that logs are not spammed without a reason.
            tracing::debug!("Aggregated application health: {this:?}");
        }
        this
    }

    async fn check_health_with_time_limit(check: &dyn CheckHealth) -> (&'static str, Health) {
        const WARNING_TIME_LIMIT: Duration = Duration::from_secs(3);
        /// Chosen to be lesser than a typical HTTP client timeout (~30s).
        const HARD_TIME_LIMIT: Duration = Duration::from_secs(20);

        let check_name = check.name();
        let timeout_at = tokio::time::Instant::now() + HARD_TIME_LIMIT;
        let mut check_future = check.check_health();
        match tokio::time::timeout(WARNING_TIME_LIMIT, &mut check_future).await {
            Ok(output) => return (check_name, output),
            Err(_) => {
                tracing::info!(
                    "Health check `{check_name}` takes >{WARNING_TIME_LIMIT:?} to complete"
                );
            }
        }

        match tokio::time::timeout_at(timeout_at, check_future).await {
            Ok(output) => (check_name, output),
            Err(_) => {
                tracing::warn!(
                    "Health check `{check_name}` timed out, taking >{HARD_TIME_LIMIT:?} to complete; marking as not ready"
                );
                (check_name, HealthStatus::NotReady.into())
            }
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.inner.status.is_healthy()
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

impl fmt::Debug for dyn CheckHealth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CheckHealth")
            .field("name", &self.name())
            .finish()
    }
}

/// Basic implementation of [`CheckHealth`] trait that can be updated using a matching [`HealthUpdater`].
#[derive(Debug, Clone)]
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
            should_track_drop: true,
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
/// if the dropping thread is panicking, unless the drop is performed using [`Self::freeze()`].
#[derive(Debug)]
pub struct HealthUpdater {
    name: &'static str,
    should_track_drop: bool,
    health_sender: watch::Sender<Health>,
}

impl HealthUpdater {
    /// Updates the health check information, returning if a change occurred from previous state.
    /// Note, description change on Health is counted as a change, even if status is the same.
    /// I.e., `Health { Ready, None }` to `Health { Ready, Some(_) }` is considered a change.
    pub fn update(&self, health: Health) -> bool {
        let old_health = self.health_sender.send_replace(health.clone());
        if old_health != health {
            tracing::debug!(
                "Changed health of `{}` from {} to {}",
                self.name,
                serde_json::to_string(&old_health).unwrap_or_else(|_| format!("{old_health:?}")),
                serde_json::to_string(&health).unwrap_or_else(|_| format!("{health:?}"))
            );
            return true;
        }
        false
    }

    /// Closes this updater so that the corresponding health check can no longer be updated, not even if the updater is dropped.
    pub fn freeze(mut self) {
        self.should_track_drop = false;
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
        if !self.should_track_drop {
            return;
        }

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
    async fn updating_health_status_after_freeze() {
        let (health_check, health_updater) = ReactiveHealthCheck::new("test");
        health_updater.update(HealthStatus::Ready.into());
        health_updater.freeze();

        assert_matches!(
            health_check.check_health().await.status(),
            HealthStatus::Ready
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

    #[tokio::test]
    async fn aggregating_health_checks() {
        let (first_check, first_updater) = ReactiveHealthCheck::new("first");
        let (second_check, second_updater) = ReactiveHealthCheck::new("second");
        let checks: Vec<Arc<dyn CheckHealth>> = vec![Arc::new(first_check), Arc::new(second_check)];

        let app_health = AppHealth::new(&checks).await;
        assert!(!app_health.is_healthy());
        assert_matches!(app_health.inner.status(), HealthStatus::NotReady);
        assert_matches!(
            app_health.components["first"].status,
            HealthStatus::NotReady
        );
        assert_matches!(
            app_health.components["second"].status,
            HealthStatus::NotReady
        );

        first_updater.update(HealthStatus::Ready.into());

        let app_health = AppHealth::new(&checks).await;
        assert!(!app_health.is_healthy());
        assert_matches!(app_health.inner.status(), HealthStatus::NotReady);
        assert_matches!(app_health.components["first"].status, HealthStatus::Ready);
        assert_matches!(
            app_health.components["second"].status,
            HealthStatus::NotReady
        );

        second_updater.update(HealthStatus::Affected.into());

        let app_health = AppHealth::new(&checks).await;
        assert!(app_health.is_healthy());
        assert_matches!(app_health.inner.status(), HealthStatus::Affected);
        assert_matches!(app_health.components["first"].status, HealthStatus::Ready);
        assert_matches!(
            app_health.components["second"].status,
            HealthStatus::Affected
        );

        drop(first_updater);

        let app_health = AppHealth::new(&checks).await;
        assert!(!app_health.is_healthy());
        assert_matches!(app_health.inner.status(), HealthStatus::ShutDown);
        assert_matches!(
            app_health.components["first"].status,
            HealthStatus::ShutDown
        );
        assert_matches!(
            app_health.components["second"].status,
            HealthStatus::Affected
        );
    }
}
