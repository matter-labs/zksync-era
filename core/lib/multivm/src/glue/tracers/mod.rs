//! # Multivm Tracing
//!
//! The MultiVM tracing module enables support for Tracers in different versions of virtual machines.
//!
//! ## Overview
//!
//! Different VM versions may have distinct requirements and types for Tracers. To accommodate these differences,
//! this module defines one primary trait:
//!
//! - `MultivmTracer<S, H>`: This trait represents a tracer that can be converted into a tracer for
//! a specific VM version.
//!
//! Specific traits for each VM version, which support Custom Tracers:
//! - `IntoLatestTracer<S, H>`: This trait is responsible for converting a tracer
//! into a form compatible with the latest VM version.
//! It defines a method `latest` for obtaining a boxed tracer.
//!
//! - `IntoVmVirtualBlocksTracer<S, H>`: This trait is responsible for converting a tracer
//! into a form compatible with the vm_virtual_blocks version.
//! It defines a method `vm_virtual_blocks` for obtaining a boxed tracer.
//!
//! For `MultivmTracer` to be implemented, the Tracer must implement all N currently
//! existing sub-traits.
//!
//! ## Adding a new VM version
//!
//! To add support for one more VM version to MultivmTracer, one needs to:
//! - Create a new trait performing conversion to the specified VM tracer, e.g., `Into<VmVersion>Tracer`.
//! - Add this trait as a trait bound to the `MultivmTracer`.
//! - Add this trait as a trait bound for `T` in `MultivmTracer` implementation.
//! — Implement the trait for `T` with a bound to `VmTracer` for a specific version.
//!
use crate::HistoryMode;
use zksync_state::WriteStorage;

pub type MultiVmTracerPointer<S, H> = Box<dyn MultivmTracer<S, H>>;

pub trait MultivmTracer<S: WriteStorage, H: HistoryMode>:
    IntoLatestTracer<S, H> + IntoVmVirtualBlocksTracer<S, H> + IntoVmRefundsEnhancementTracer<S, H>
{
    fn into_tracer_pointer(self) -> MultiVmTracerPointer<S, H>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

pub trait IntoLatestTracer<S: WriteStorage, H: HistoryMode> {
    fn latest(&self) -> crate::vm_latest::TracerPointer<S, H::VmBoojumIntegration>;
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

impl<S, T, H> IntoLatestTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: crate::vm_latest::VmTracer<S, H::VmBoojumIntegration> + Clone + 'static,
{
    fn latest(&self) -> crate::vm_latest::TracerPointer<S, H::VmBoojumIntegration> {
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

impl<S, H, T> MultivmTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: IntoLatestTracer<S, H>
        + IntoVmVirtualBlocksTracer<S, H>
        + IntoVmRefundsEnhancementTracer<S, H>,
{
}
