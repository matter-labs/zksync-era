//! `Network` and related types.

use std::fmt;

use zksync_types::{L1ChainId, L2ChainId};

/// Marker trait for networks. Two standard network kinds are [`L1`] and [`L2`].
///
/// The `Default` value should belong to a "generic" / "unknown" network, rather than to a specific network like the mainnet.
pub trait Network: 'static + Copy + Default + Sync + Send + fmt::Debug {
    /// String representation of a network used as a metric label and in log messages.
    fn metric_label(&self) -> String;
}

/// L1 (i.e., Ethereum) network.
#[derive(Debug, Clone, Copy, Default)]
pub struct L1(Option<L1ChainId>);

impl Network for L1 {
    fn metric_label(&self) -> String {
        if let Some(chain_id) = self.0 {
            format!("ethereum_{chain_id}")
        } else {
            "ethereum".to_owned()
        }
    }
}

impl From<L1ChainId> for L1 {
    fn from(chain_id: L1ChainId) -> Self {
        Self(Some(chain_id))
    }
}

/// L2 network.
#[derive(Debug, Clone, Copy, Default)]
pub struct L2(Option<L2ChainId>);

impl Network for L2 {
    fn metric_label(&self) -> String {
        if let Some(chain_id) = self.0 {
            format!("l2_{}", chain_id.as_u64())
        } else {
            "l2".to_owned()
        }
    }
}

impl From<L2ChainId> for L2 {
    fn from(chain_id: L2ChainId) -> Self {
        Self(Some(chain_id))
    }
}

/// Associates a type with a particular type of RPC networks, such as Ethereum or ZKsync Era. RPC traits created using `jsonrpsee::rpc`
/// can use `ForNetwork` as a client boundary to restrict which implementations can call their methods.
pub trait ForNetwork {
    /// Network that the type is associated with.
    type Net: Network;

    /// Returns a network for this type instance.
    fn network(&self) -> Self::Net;

    /// Returns the component tag. The component name can be used in logging, metrics etc.
    /// The component name should be copied to the clones of this client, but should not be passed upstream.
    fn component(&self) -> &'static str;
}

impl<T: ?Sized + ForNetwork> ForNetwork for &T {
    type Net = T::Net;

    fn network(&self) -> Self::Net {
        (**self).network()
    }

    fn component(&self) -> &'static str {
        (**self).component()
    }
}

impl<T: ?Sized + ForNetwork> ForNetwork for Box<T> {
    type Net = T::Net;

    fn network(&self) -> Self::Net {
        self.as_ref().network()
    }

    fn component(&self) -> &'static str {
        self.as_ref().component()
    }
}

/// Client that can be tagged with the component using it.
pub trait TaggedClient: ForNetwork {
    /// Tags this client as working for a specific component.
    fn set_component(&mut self, component_name: &'static str);
}
