//! Miscellaneous macros used across project.

/// Implements `From` trait for given types allowing to convert wrapper to inner and vice versa.
/// If prefix `deref` supplied, also implements `From` trait for references.
#[macro_export]
macro_rules! impl_from_wrapper {
    ($wrapper: ty, $inner: ty $(where for $(<$($gen: ident),+>)?: $($where: tt)+)?) => {
        impl $($(<$($gen),+>)*)? From<$inner> for $wrapper $(where $($where)+)? {
            fn from(inner: $inner) -> Self {
                Self(inner)
            }
        }

        impl $($(<$($gen),+>)*)? From<$wrapper> for $inner $(where $($where)+)? {
            fn from(wrapper: $wrapper) -> Self {
                wrapper.0
            }
        }
    };
    (deref $wrapper: ty, $inner: ty $(where for $(<$($gen: ident),+>)?: $($where: tt)+)?) => {
        $crate::impl_from_wrapper!($wrapper, $inner $(where for $(<$($gen),+>)*: $($where)+)?);

        impl $($(<$($gen),+>)*)? From<&$inner> for $wrapper $(where $($where)+)? {
            fn from(inner: &$inner) -> Self {
                Self(*inner)
            }
        }

        impl $($(<$($gen),+>)*)? From<&$wrapper> for $inner $(where $($where)+)? {
            fn from(wrapper: &$wrapper) -> Self {
                (*wrapper).0
            }
        }
    };
}
