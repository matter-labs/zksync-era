//! Calibration-feature types for the Airbender cycle estimator.
//!
//! The types themselves now live in `zksync_vm_interface` (see
//! [`zksync_vm_interface::FeatureVector`]) so they can ride in
//! `VmExecutionStatistics`, be accumulated across a batch by the state keeper, and
//! be read by a seal criterion without those layers depending on `multivm`. This
//! module re-exports them so the tracer, the vendored estimator/model, and their
//! tests keep referring to `crate::tracers::cycle_estimator::features`.

pub use zksync_vm_interface::{FeatureId, FeatureVector, SAFETY_CRITICAL_FEATURES};
