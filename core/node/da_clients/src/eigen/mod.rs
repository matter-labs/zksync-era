mod client;
mod sdk;

pub use self::client::EigenClient;

pub(crate) mod disperser {
    include!("generated/disperser.rs");
}

pub(crate) mod common {
    include!("generated/common.rs");
}
