#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressedBytecodeInfo {
    pub original: Vec<u8>,
    pub compressed: Vec<u8>,
}
