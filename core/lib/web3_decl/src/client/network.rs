//! `Network` and related types.

use std::fmt;

use zksync_types::{L1ChainId, L2ChainId};

/// Marker trait for networks. Two standard network kinds are [`L1`] and [`L2`].
///
/// The `Display` implementation will be used in logging.
pub trait Network: 'static + Copy + Sync + Send + fmt::Debug {
    fn metric_label(&self) -> String;
}

/// L1 (i.e., Ethereum) network.
#[derive(Debug, Clone, Copy)]
pub struct L1(pub L1ChainId);

impl Network for L1 {
    fn metric_label(&self) -> String {
        format!("ethereum_{}", self.0)
    }
}

/// L2 (i.e., zkSync Era) network.
#[derive(Debug, Clone, Copy, Default)]
pub struct L2(pub L2ChainId);

impl Network for L2 {
    fn metric_label(&self) -> String {
        format!("l2_{}", self.0.as_u64())
    }
}

/// Associates a type with a particular RPC network, such as Ethereum or zkSync Era. RPC traits created using `jsonrpseee::rpc`
/// can use `ForNetwork` as a client boundary to restrict which implementations can call their methods.
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

/// Client that can be tagged with the component using it.
pub trait TaggedClient: ForNetwork {
    /// Tags this client as working for a specific component. The component name can be used in logging,
    /// metrics etc. The component name should be copied to the clones of this client, but should not be passed upstream.
    fn for_component(self, component_name: &'static str) -> Self;

    /// Returns the component tag.
    fn component(&self) -> &'static str;
}
