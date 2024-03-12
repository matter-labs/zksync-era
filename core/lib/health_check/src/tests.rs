//! Tests for health checks.

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
    let checks = AppHealthCheck {
        components: Mutex::new(vec![Arc::new(first_check), Arc::new(second_check)]),
        ..AppHealthCheck::default()
    };

    let app_health = checks.check_health().await;
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

    let app_health = checks.check_health().await;
    assert!(!app_health.is_healthy());
    assert_matches!(app_health.inner.status(), HealthStatus::NotReady);
    assert_matches!(app_health.components["first"].status, HealthStatus::Ready);
    assert_matches!(
        app_health.components["second"].status,
        HealthStatus::NotReady
    );

    second_updater.update(HealthStatus::Affected.into());

    let app_health = checks.check_health().await;
    assert!(app_health.is_healthy());
    assert_matches!(app_health.inner.status(), HealthStatus::Affected);
    assert_matches!(app_health.components["first"].status, HealthStatus::Ready);
    assert_matches!(
        app_health.components["second"].status,
        HealthStatus::Affected
    );

    drop(first_updater);

    let app_health = checks.check_health().await;
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
