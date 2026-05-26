use zksync_basic_types::{protocol_version::ProtocolVersionId, U256};

pub fn get_minor_protocol_version(protocol_version: U256) -> anyhow::Result<ProtocolVersionId> {
    ProtocolVersionId::try_from_packed_semver(protocol_version)
        .map_err(|err| anyhow::format_err!("Failed to unpack semver for protocol version: {err}"))
}
