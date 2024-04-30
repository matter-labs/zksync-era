//! Configuration metadata.

use std::{any, fmt, num};

/// Describes a configuration (i.e., a group of related parameters).
pub trait DescribeConfig {
    /// Provides the description.
    fn describe_config() -> ConfigMetadata;
}

/// Metadata for a configuration (i.e., a group of related parameters).
#[derive(Debug)]
pub struct ConfigMetadata {
    /// Parameters included in the config.
    pub params: Vec<ParamMetadata>,
}

/// Metadata for a specific configuration parameter.
#[derive(Debug)]
pub struct ParamMetadata {
    pub name: &'static str,
    pub aliases: Vec<&'static str>,
    pub help: &'static str,
    pub ty: RustType,
    pub base_type: RustType,
    pub unit: Option<UnitOfMeasurement>,
    pub default_value: Option<Box<dyn fmt::Debug>>,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum UnitOfMeasurement {
    Seconds,
    Milliseconds,
    Megabytes,
}

impl UnitOfMeasurement {
    pub fn detect(param_name: &str, base_type: RustType) -> Option<Self> {
        if base_type.kind() != Some(TypeKind::Integer) {
            return None;
        }

        if param_name.ends_with("_ms") {
            Some(Self::Milliseconds)
        } else if param_name.ends_with("_sec") {
            Some(Self::Seconds)
        } else if param_name.ends_with("_mb") {
            Some(Self::Megabytes)
        } else {
            None
        }
    }
}

/// Represenation of a Rust type.
#[derive(Debug, Clone, Copy)]
pub struct RustType {
    id: any::TypeId,
    name_in_code: &'static str,
    canonical_name: &'static str,
}

impl RustType {
    pub fn of<T: 'static>(name_in_code: &'static str) -> Self {
        Self {
            id: any::TypeId::of::<T>(),
            name_in_code,
            canonical_name: any::type_name::<T>(),
        }
    }

    pub fn name_in_code(&self) -> &'static str {
        self.name_in_code
    }

    pub fn canonical_name(self) -> &'static str {
        self.canonical_name
    }

    ///
    pub fn kind(self) -> Option<TypeKind> {
        match self.id {
            id if id == any::TypeId::of::<bool>() => Some(TypeKind::Bool),
            id if id == any::TypeId::of::<u8>()
                || id == any::TypeId::of::<i8>()
                || id == any::TypeId::of::<u16>()
                || id == any::TypeId::of::<i16>()
                || id == any::TypeId::of::<u32>()
                || id == any::TypeId::of::<i32>()
                || id == any::TypeId::of::<u64>()
                || id == any::TypeId::of::<i64>()
                || id == any::TypeId::of::<usize>()
                || id == any::TypeId::of::<isize>()
                || id == any::TypeId::of::<num::NonZeroU8>()
                || id == any::TypeId::of::<num::NonZeroI8>()
                || id == any::TypeId::of::<num::NonZeroU16>()
                || id == any::TypeId::of::<num::NonZeroI16>()
                || id == any::TypeId::of::<num::NonZeroU32>()
                || id == any::TypeId::of::<num::NonZeroI32>()
                || id == any::TypeId::of::<num::NonZeroU64>()
                || id == any::TypeId::of::<num::NonZeroI64>()
                || id == any::TypeId::of::<num::NonZeroUsize>()
                || id == any::TypeId::of::<num::NonZeroIsize>() =>
            {
                Some(TypeKind::Integer)
            }
            id if id == any::TypeId::of::<String>() => Some(TypeKind::String),
            _ => None,
        }
    }
}

/// Human-readable kind for a Rust type used in configuration parameter (Boolean value, integer, string etc.).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum TypeKind {
    Bool,
    Integer,
    String,
}
