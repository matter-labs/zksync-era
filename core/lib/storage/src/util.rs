use byteorder::ReadBytesExt;
use byteorder::{ByteOrder, LittleEndian};
use zksync_types::U256;

pub fn serialize_block_number(block_number: u32) -> Vec<u8> {
    let mut bytes = vec![0; 4];
    LittleEndian::write_u32(&mut bytes, block_number);
    bytes
}

pub fn deserialize_block_number(mut bytes: &[u8]) -> u32 {
    bytes
        .read_u32::<LittleEndian>()
        .expect("failed to deserialize block number")
}

pub fn serialize_tree_leaf(leaf: U256) -> Vec<u8> {
    let mut bytes = vec![0; 32];
    leaf.to_big_endian(&mut bytes);
    bytes
}
