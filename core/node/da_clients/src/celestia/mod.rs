mod client;
mod sdk;

pub use self::client::CelestiaClient;

pub mod celestia_proto {
    include!("generated/celestia.blob.v1.rs");
}

pub mod cosmos {
    pub mod auth {
        include!("generated/cosmos.auth.v1beta1.rs");
    }

    pub mod base {
        pub mod abci {
            include!("generated/cosmos.base.abci.v1beta1.rs");
        }

        pub mod node {
            include!("generated/cosmos.base.node.v1beta1.rs");
        }

        pub mod v1beta1 {
            include!("generated/cosmos.base.v1beta1.rs");
        }

        pub mod query {
            include!("generated/cosmos.base.query.v1beta1.rs");
        }
    }

    pub mod bank {
        pub mod v1beta1 {
            include!("generated/cosmos.bank.v1beta1.rs");
        }
    }

    pub mod tx {
        pub mod signing {
            include!("generated/cosmos.tx.signing.v1beta1.rs");
        }

        pub mod v1beta1 {
            include!("generated/cosmos.tx.v1beta1.rs");
        }
    }

    pub mod crypto {
        pub mod multisig {
            include!("generated/cosmos.crypto.multisig.v1beta1.rs");
        }

        pub mod secp256k1 {
            include!("generated/cosmos.crypto.secp256k1.rs");
        }
    }
}

pub mod tendermint {
    pub mod abci {
        include!("generated/tendermint.abci.rs");
    }

    pub mod types {
        include!("generated/tendermint.types.rs");
    }
}
