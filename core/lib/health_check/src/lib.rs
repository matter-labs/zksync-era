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

use self::metrics::METRICS;
use crate::metrics::CheckResult;

mod metrics;
#[cfg(test)]
mod tests;

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
    /// Component has received a termination request and is in the process of shutting down.
    /// Components that shut down instantly may skip this status and proceed directly to [`Self::ShutDown`].
    ShuttingDown,
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
            Self::ShuttingDown => 2,
            Self::ShutDown => 3,
            Self::NotReady => 4,
            Self::Panicked => 5,
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
#[derive(Debug)]
pub struct AppHealthCheck {
    components: Mutex<Vec<Arc<dyn CheckHealth>>>,
    slow_time_limit: Duration,
    hard_time_limit: Duration,
}

impl Default for AppHealthCheck {
    fn default() -> Self {
        Self::new(None, None)
    }
}

impl AppHealthCheck {
    pub fn new(slow_time_limit: Option<Duration>, hard_time_limit: Option<Duration>) -> Self {
        const DEFAULT_SLOW_TIME_LIMIT: Duration = Duration::from_millis(500);
        const DEFAULT_HARD_TIME_LIMIT: Duration = Duration::from_secs(3);

        let slow_time_limit = slow_time_limit.unwrap_or(DEFAULT_SLOW_TIME_LIMIT);
        let hard_time_limit = hard_time_limit.unwrap_or(DEFAULT_HARD_TIME_LIMIT);
        tracing::debug!("Created app health with time limits: slow={slow_time_limit:?}, hard={hard_time_limit:?}");
        Self {
            components: Mutex::default(),
            slow_time_limit,
            hard_time_limit,
        }
    }

    /// Inserts health check for a component.
    pub fn insert_component(&self, health_check: ReactiveHealthCheck) {
        self.insert_custom_component(Arc::new(health_check));
    }

    /// Inserts a custom health check for a component.
    pub fn insert_custom_component(&self, health_check: Arc<dyn CheckHealth>) {
        let health_check_name = health_check.name();
        let mut guard = self
            .components
            .lock()
            .expect("`AppHealthCheck` is poisoned");
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
        let health_checks = self
            .components
            .lock()
            .expect("`AppHealthCheck` is poisoned")
            .clone();

        let check_futures = health_checks.iter().map(|check| {
            Self::check_health_with_time_limit(
                check.as_ref(),
                self.slow_time_limit,
                self.hard_time_limit,
            )
        });
        let components: HashMap<_, _> = future::join_all(check_futures).await.into_iter().collect();

        let aggregated_status = components
            .values()
            .map(|health| health.status)
            .max_by_key(|status| status.priority_for_aggregation())
            .unwrap_or(HealthStatus::Ready);
        let inner = aggregated_status.into();

        let health = AppHealth { inner, components };
        if !health.inner.status.is_healthy() {
            // Only log non-ready application health so that logs are not spammed without a reason.
            tracing::debug!("Aggregated application health: {health:?}");
        }
        health
    }

    async fn check_health_with_time_limit(
        check: &dyn CheckHealth,
        slow_time_limit: Duration,
        hard_time_limit: Duration,
    ) -> (&'static str, Health) {
        struct DropGuard {
            check_name: &'static str,
            started_at: tokio::time::Instant,
            hard_time_limit: Duration,
            is_armed: bool,
        }

        impl Drop for DropGuard {
            fn drop(&mut self) {
                if !self.is_armed {
                    return;
                }

                let elapsed = self.started_at.elapsed();
                let &mut Self {
                    check_name,
                    hard_time_limit,
                    ..
                } = self;
                tracing::warn!(
                    "Health check `{check_name}` was dropped before completion after {elapsed:?}; \
                     check the configured check timeout ({hard_time_limit:?}) and health check logic"
                );
                METRICS.observe_abnormal_check(check_name, CheckResult::Dropped, elapsed);
            }
        }

        let check_name = check.name();
        let started_at = tokio::time::Instant::now();
        let mut drop_guard = DropGuard {
            check_name,
            started_at,
            hard_time_limit,
            is_armed: true,
        };
        let timeout_at = started_at + hard_time_limit;

        let result = tokio::time::timeout_at(timeout_at, check.check_health()).await;
        drop_guard.is_armed = false;
        let elapsed = started_at.elapsed();
        match result {
            Ok(output) => {
                if elapsed > slow_time_limit {
                    tracing::info!(
                        "Health check `{check_name}` took >{slow_time_limit:?} to complete: {elapsed:?}"
                    );
                    METRICS.observe_abnormal_check(check_name, CheckResult::Slow, elapsed);
                }
                (check_name, output)
            }
            Err(_) => {
                tracing::warn!(
                    "Health check `{check_name}` timed out, taking >{hard_time_limit:?} to complete; marking as not ready"
                );
                METRICS.observe_abnormal_check(check_name, CheckResult::TimedOut, elapsed);
                (check_name, HealthStatus::NotReady.into())
            }
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
            HealthStatus::Panicked
        } else {
            HealthStatus::ShutDown
        };
        self.update(terminal_health.into());
    }
}
