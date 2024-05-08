//! `Network` and related types.

/// Marker trait for network types. Two standard networks are [`L1`] and [`L2`].
pub trait Network: 'static + Copy + Sync + Send {}

/// L1 (i.e., Ethereum) network.
#[derive(Debug, Clone, Copy)]
pub struct L1(());

impl Network for L1 {}

/// L2 (i.e., zkSync Era) network.
#[derive(Debug, Clone, Copy)]
pub struct L2(());

impl Network for L2 {}

/// Associates a type with a particular network.
pub trait ForNetwork {
    /// Network that the type is associated with.
    type Net: Network;
}

impl<T: ?Sized + ForNetwork> ForNetwork for &T {
    type Net = T::Net;
}

impl<T: ?Sized + ForNetwork> ForNetwork for Box<T> {
    type Net = T::Net;
}
