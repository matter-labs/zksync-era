use zksync_basic_types::protocol_version::ProtocolVersionId;

/// Returns the number of blobs supported by each VM version
pub fn num_blobs_required(protocol_version: &ProtocolVersionId) -> usize {
    if protocol_version.is_pre_1_4_1() {
        0
    } else if protocol_version.is_pre_1_5_0() {
        2
    } else {
        16
    }
}

/// Returns the number of blobs created within each VM version
pub fn num_blobs_created(protocol_version: &ProtocolVersionId) -> usize {
    if protocol_version.is_pre_1_4_2() {
        0
    } else if protocol_version.is_pre_1_5_0() {
        2
    } else {
        9
    }
}
