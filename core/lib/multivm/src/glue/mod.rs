//! This module contains "glue" code that allows to operate with multiple versions of the VM crate families (i.e.
//! `vm` crate and its dependencies that are used in the crate API).
//!
//! Glue generally comes in two flavors:
//! - "Public glue", aka types that are used externally to instantiate the MultiVM (like `OracleTools` and `init_vm`).
//! - "Private glue", aka type conversions from current to the "past" and vice versa.
//!
//! The "private glue" lies in the `types` module.

pub(crate) mod history_mode;
pub mod tracers;
mod types;

/// This trait is a workaround on the Rust'c [orphan rule](orphan_rule).
/// We need to convert a lot of types that come from two different versions of some crate,
/// and `From`/`Into` traits are natural way of doing so. Unfortunately, we can't implement an
/// external trait on a pair of external types, so we're unable to use these traits.
///
/// However, we can implement any *local* trait on a pair of external types, so here are the "glued"
/// versions of `From`/`Into`.
///
/// [orphan_rule]: https://github.com/Ixrec/rust-orphan-rules
pub trait GlueFrom<T>: Sized {
    fn glue_from(value: T) -> Self;
}

/// See the description of [`GlueFrom`] trait above.
pub trait GlueInto<T>: Sized {
    fn glue_into(self) -> T;
}

// Blaknet `GlueInto` impl for any type that implements `GlueFrom`.
impl<T, U> GlueInto<U> for T
where
    U: GlueFrom<T>,
{
    fn glue_into(self) -> U {
        U::glue_from(self)
    }
}

// Identity impl.
impl<T> GlueFrom<T> for T {
    fn glue_from(this: T) -> Self {
        this
    }
}
