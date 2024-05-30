use zksync_basic_types::{L1ChainId, L2ChainId};
use zksync_config::configs::en_config::ENConfig;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::en as proto;

impl ProtoRepr for proto::ExternalNode {
    type Type = ENConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            main_node_url: required(&self.main_node_url)?.to_string(),
            l1_chain_id: required(&self.l1_chain_id)
                .map(|x| L1ChainId(*x))
                .context("l1_chain_id")?,
            l2_chain_id: required(&self.l2_chain_id)
                .and_then(|x| L2ChainId::try_from(*x).map_err(|a| anyhow::anyhow!(a)))
                .context("l2_chain_id")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            main_node_url: Some(this.main_node_url.clone()),
            l1_chain_id: Some(this.l1_chain_id.0),
            l2_chain_id: Some(this.l2_chain_id.as_u64()),
        }
    }
}
