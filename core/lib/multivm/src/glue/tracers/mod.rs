//! # Multivm Tracing
//!
//! The MultiVM tracing module enables Tracers support for different versions of virtual machines
//!
//! ## Overview
//!
//! Different VM versions may have distinct requirements and types for Tracers. To accommodate these differences,
//! this module defines one primary trait:
//!
//! - `MultivmTracer<S, H>`: This trait represents a tracer that can be converted into a tracer for
//! specific VM version.
//!
//! Specific traits for each VM version, which supports Custom Tracers
//! - `IntoLatestTracer<S, H>`: This trait is responsible for converting a tracer
//! into a form compatible with the latest VM version.
//! It defines a method `latest` for obtaining a boxed tracer.
//!
//! - `IntoVmVirtualBlocksTracer<S, H>`:This trait is responsible for converting a tracer
//! into a form compatible with the vm_virtual_blocks version.
//! It defines a method `vm_virtual_blocks` for obtaining a boxed tracer.
//!
//! For `MultivmTracer` to be implemented, Tracer must implement all N currently
//! existing sub-traits.
//!
//! Any tracer compatible with the current VM automatically
//! supports conversion into the latest VM,
//! but the remaining traits required by the MultivmTracer should be implemented manually.
//! If a certain tracer is not intended to be used with VMs other than the latest one,
//! these traits should still be implemented, but one may just write a panicking implementation.
//!
//! ## Adding a new VM version
//!
//! To add support for one more VM version to MultivmTracer, one needs to:
//! - Create a new trait performing conversion to the specified VM tracer, e.g. `Into<VmVersion>Tracer`.
//! - Provide implementations of this trait for all the structures that currently implement `MultivmTracer`.
//! - Add this trait as a trait bound to the `MultivmTracer`.
//! - Add this trait as a trait bound for `T` in `MultivmTracer` implementation.
//! - Integrate the newly added method to the MultiVM itself (e.g. add required tracer conversions where applicable).
use crate::HistoryMode;
use std::cell::RefCell;
use std::rc::Rc;
use zksync_state::WriteStorage;

pub trait MultivmTracer<S: WriteStorage, H: HistoryMode>:
    IntoLatestTracer<S, H> + IntoVmVirtualBlocksTracer<S, H>
{
    fn into_boxed(self) -> Box<dyn MultivmTracer<S, H>>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

pub trait IntoLatestTracer<S: WriteStorage, H: HistoryMode> {
    fn latest(&self) -> crate::vm_latest::TracerPointer<S, H::VmVirtualBlocksRefundsEnhancement>;
}

pub trait IntoVmVirtualBlocksTracer<S: WriteStorage, H: HistoryMode> {
    fn vm_virtual_blocks(
        &self,
    ) -> crate::vm_virtual_blocks::TracerPointer<S, H::VmVirtualBlocksMode>;
}

impl<S, T, H> IntoLatestTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: crate::vm_latest::VmTracer<S, H::VmVirtualBlocksRefundsEnhancement> + Clone + 'static,
{
    fn latest(&self) -> crate::vm_latest::TracerPointer<S, H::VmVirtualBlocksRefundsEnhancement> {
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
        Rc::new(RefCell::new(self.clone()))
    }
}

impl<S, H, T> MultivmTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: IntoLatestTracer<S, H> + IntoVmVirtualBlocksTracer<S, H>,
{
}
