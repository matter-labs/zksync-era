//! # MultiVM Tracing
//!
//! The MultiVM tracing module enables support for Tracers in different versions of virtual machines.
//!
//! ## Overview
//!
//! Different VM versions may have distinct requirements and types for Tracers. To accommodate these differences,
//! this module defines one primary trait:
//!
//! - `MultiVmTracer<S, H>`: This trait represents a tracer that can be converted into a tracer for
//!   a specific VM version.
//!
//! Specific traits for each VM version, which support Custom Tracers:
//! - `IntoLatestTracer<S, H>`: This trait is responsible for converting a tracer
//!   into a form compatible with the latest VM version.
//!   It defines a method `latest` for obtaining a boxed tracer.
//!
//! - `IntoVmVirtualBlocksTracer<S, H>`: This trait is responsible for converting a tracer
//!   into a form compatible with the vm_virtual_blocks version.
//!   It defines a method `vm_virtual_blocks` for obtaining a boxed tracer.
//!
//! For `MultiVmTracer` to be implemented, the Tracer must implement all N currently
//! existing sub-traits.
//!
//! ## Adding a new VM version
//!
//! To add support for one more VM version to MultiVmTracer, one needs to:
//! - Create a new trait performing conversion to the specified VM tracer, e.g., `Into<VmVersion>Tracer`.
//! - Add this trait as a trait bound to the `MultiVmTracer`.
//! - Add this trait as a trait bound for `T` in `MultiVmTracer` implementation.
//! - Implement the trait for `T` with a bound to `VmTracer` for a specific version.

use crate::{interface::storage::WriteStorage, tracers::old::OldTracers, HistoryMode};

pub type MultiVmTracerPointer<S, H> = Box<dyn MultiVmTracer<S, H>>;

pub trait MultiVmTracer<S: WriteStorage, H: HistoryMode>:
    IntoLatestTracer<S, H>
    + IntoVmVirtualBlocksTracer<S, H>
    + IntoVmRefundsEnhancementTracer<S, H>
    + IntoVmBoojumIntegrationTracer<S, H>
    + IntoVm1_4_1IntegrationTracer<S, H>
    + IntoVm1_4_2IntegrationTracer<S, H>
    + IntoOldVmTracer
{
    fn into_tracer_pointer(self) -> MultiVmTracerPointer<S, H>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

pub trait IntoLatestTracer<S: WriteStorage, H: HistoryMode> {
    fn latest(&self) -> crate::vm_latest::TracerPointer<S, H::Vm1_5_2>;
}

pub trait IntoVmVirtualBlocksTracer<S: WriteStorage, H: HistoryMode> {
    fn vm_virtual_blocks(
        &self,
    ) -> crate::vm_virtual_blocks::TracerPointer<S, H::VmVirtualBlocksMode>;
}

pub trait IntoVmRefundsEnhancementTracer<S: WriteStorage, H: HistoryMode> {
    fn vm_refunds_enhancement(
        &self,
    ) -> Box<dyn crate::vm_refunds_enhancement::VmTracer<S, H::VmVirtualBlocksRefundsEnhancement>>;
}

pub trait IntoVmBoojumIntegrationTracer<S: WriteStorage, H: HistoryMode> {
    fn vm_boojum_integration(
        &self,
    ) -> Box<dyn crate::vm_boojum_integration::VmTracer<S, H::VmBoojumIntegration>>;
}

pub trait IntoVm1_4_1IntegrationTracer<S: WriteStorage, H: HistoryMode> {
    fn vm_1_4_1(&self) -> Box<dyn crate::vm_1_4_1::VmTracer<S, H::Vm1_4_1>>;
}

pub trait IntoVm1_4_2IntegrationTracer<S: WriteStorage, H: HistoryMode> {
    fn vm_1_4_2(&self) -> Box<dyn crate::vm_1_4_2::VmTracer<S, H::Vm1_4_2>>;
}

/// Into tracers for old VM versions.
///
/// Even though number of tracers is limited, we still need to have this trait to be able to convert
/// tracers to old VM tracers.
/// Unfortunately we can't implement this trait for `T`, because specialization is not stable yet.
/// You can follow the conversation here: https://github.com/rust-lang/rust/issues/31844
/// For all new tracers we need to implement this trait manually.
pub trait IntoOldVmTracer {
    fn old_tracer(&self) -> OldTracers {
        OldTracers::None
    }
}

impl<S, T, H> IntoLatestTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: crate::vm_latest::VmTracer<S, H::Vm1_5_2> + Clone + 'static,
{
    fn latest(&self) -> crate::vm_latest::TracerPointer<S, H::Vm1_5_2> {
        Box::new(self.clone())
    }
}

impl<S, T, H> IntoVmVirtualBlocksTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: crate::vm_virtual_blocks::VmTracer<S, H::VmVirtualBlocksMode> + Clone + 'static,
{
    fn vm_virtual_blocks(
        &self,
    ) -> crate::vm_virtual_blocks::TracerPointer<S, H::VmVirtualBlocksMode> {
        Box::new(self.clone())
    }
}

impl<S, T, H> IntoVmRefundsEnhancementTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: crate::vm_refunds_enhancement::VmTracer<S, H::VmVirtualBlocksRefundsEnhancement>
        + Clone
        + 'static,
{
    fn vm_refunds_enhancement(
        &self,
    ) -> Box<dyn crate::vm_refunds_enhancement::VmTracer<S, H::VmVirtualBlocksRefundsEnhancement>>
    {
        Box::new(self.clone())
    }
}

impl<S, T, H> IntoVmBoojumIntegrationTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: crate::vm_boojum_integration::VmTracer<S, H::VmBoojumIntegration> + Clone + 'static,
{
    fn vm_boojum_integration(
        &self,
    ) -> Box<dyn crate::vm_boojum_integration::VmTracer<S, H::VmBoojumIntegration>> {
        Box::new(self.clone())
    }
}

impl<S, T, H> IntoVm1_4_1IntegrationTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: crate::vm_1_4_1::VmTracer<S, H::Vm1_4_1> + Clone + 'static,
{
    fn vm_1_4_1(&self) -> Box<dyn crate::vm_1_4_1::VmTracer<S, H::Vm1_4_1>> {
        Box::new(self.clone())
    }
}

impl<S, T, H> IntoVm1_4_2IntegrationTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: crate::vm_1_4_2::VmTracer<S, H::Vm1_4_2> + Clone + 'static,
{
    fn vm_1_4_2(&self) -> Box<dyn crate::vm_1_4_2::VmTracer<S, <H as HistoryMode>::Vm1_4_2>> {
        Box::new(self.clone())
    }
}

impl<S, H, T> MultiVmTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: IntoLatestTracer<S, H>
        + IntoVmVirtualBlocksTracer<S, H>
        + IntoVmRefundsEnhancementTracer<S, H>
        + IntoVmBoojumIntegrationTracer<S, H>
        + IntoVm1_4_1IntegrationTracer<S, H>
        + IntoVm1_4_2IntegrationTracer<S, H>
        + IntoOldVmTracer,
{
}
