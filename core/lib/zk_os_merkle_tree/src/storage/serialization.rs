//! Binary serialization of tree nodes.

use std::str;

use zksync_basic_types::H256;

use crate::{
    errors::{DeserializeContext, DeserializeErrorKind},
    storage::InsertedKeyEntry,
    types::{ChildRef, InternalNode, Leaf, Manifest, RawNode, Root, TreeTags},
    DeserializeError,
};

const HASH_SIZE: usize = 32;

impl InsertedKeyEntry {
    pub(super) fn deserialize(mut buffer: &[u8]) -> Result<Self, DeserializeError> {
        let index = leb128::read::unsigned(&mut buffer).map_err(DeserializeErrorKind::Leb128)?;
        let inserted_at =
            leb128::read::unsigned(&mut buffer).map_err(DeserializeErrorKind::Leb128)?;
        Ok(Self { index, inserted_at })
    }

    pub(super) fn serialize(&self, buffer: &mut Vec<u8>) {
        leb128::write::unsigned(buffer, self.index).unwrap();
        leb128::write::unsigned(buffer, self.inserted_at).unwrap();
    }
}

impl Leaf {
    pub(super) fn deserialize(mut buffer: &[u8]) -> Result<Self, DeserializeError> {
        if buffer.len() < 2 * HASH_SIZE {
            return Err(DeserializeErrorKind::UnexpectedEof.into());
        }
        let key = H256::from_slice(&buffer[..HASH_SIZE]);
        let value = H256::from_slice(&buffer[HASH_SIZE..2 * HASH_SIZE]);

        buffer = &buffer[2 * HASH_SIZE..];
        let next_index =
            leb128::read::unsigned(&mut buffer).map_err(DeserializeErrorKind::Leb128)?;
        if !buffer.is_empty() {
            return Err(DeserializeErrorKind::Leftovers.into());
        }
        Ok(Self {
            key,
            value,
            next_index,
        })
    }

    pub(super) fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(self.key.as_bytes());
        buffer.extend_from_slice(self.value.as_bytes());
        leb128::write::unsigned(buffer, self.next_index).unwrap();
    }
}

impl ChildRef {
    fn deserialize(buffer: &mut &[u8]) -> Result<Self, DeserializeError> {
        if buffer.len() < HASH_SIZE {
            return Err(DeserializeErrorKind::UnexpectedEof.into());
        }
        let hash = H256::from_slice(&buffer[..HASH_SIZE]);
        *buffer = &buffer[HASH_SIZE..];
        let version = leb128::read::unsigned(buffer).map_err(DeserializeErrorKind::Leb128)?;
        Ok(Self { hash, version })
    }

    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(self.hash.as_bytes());
        leb128::write::unsigned(buffer, self.version).unwrap();
    }
}

impl InternalNode {
    pub(super) fn deserialize(mut buffer: &[u8]) -> Result<Self, DeserializeError> {
        if buffer.is_empty() {
            return Err(DeserializeErrorKind::UnexpectedEof.into());
        }
        let len = buffer[0];
        buffer = &buffer[1..];

        let children: Vec<_> = (0..len)
            .map(|i| {
                ChildRef::deserialize(&mut buffer)
                    .map_err(|err| err.with_context(DeserializeContext::ChildRef(i)))
            })
            .collect::<Result<_, _>>()?;
        if !buffer.is_empty() {
            return Err(DeserializeErrorKind::Leftovers.into());
        }
        Ok(Self { children })
    }

    pub(super) fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.children.len() as u8);

        for child_ref in &self.children {
            child_ref.serialize(buffer);
        }
    }
}

impl Root {
    pub(super) fn deserialize(mut buffer: &[u8]) -> Result<Self, DeserializeError> {
        let leaf_count = leb128::read::unsigned(&mut buffer).map_err(|err| {
            DeserializeError::from(DeserializeErrorKind::Leb128(err))
                .with_context(DeserializeContext::LeafCount)
        })?;
        Ok(Self {
            leaf_count,
            root_node: InternalNode::deserialize(buffer)?,
        })
    }

    pub(super) fn serialize(&self, buffer: &mut Vec<u8>) {
        leb128::write::unsigned(buffer, self.leaf_count).unwrap();
        self.root_node.serialize(buffer);
    }
}

impl RawNode {
    pub(crate) fn deserialize(bytes: &[u8]) -> Self {
        Self {
            raw: bytes.to_vec(),
            leaf: Leaf::deserialize(bytes).ok(),
            internal: InternalNode::deserialize(bytes).ok(),
        }
    }

    pub(crate) fn deserialize_root(bytes: &[u8]) -> Self {
        let root = Root::deserialize(bytes).ok();
        Self {
            raw: bytes.to_vec(),
            leaf: None,
            internal: root.map(|root| root.root_node),
        }
    }
}

impl TreeTags {
    fn deserialize_str<'a>(bytes: &mut &'a [u8]) -> Result<&'a str, DeserializeErrorKind> {
        let str_len = leb128::read::unsigned(bytes).map_err(DeserializeErrorKind::Leb128)?;
        let str_len = usize::try_from(str_len).map_err(|_| DeserializeErrorKind::UnexpectedEof)?;

        if bytes.len() < str_len {
            return Err(DeserializeErrorKind::UnexpectedEof);
        }
        let (s, rest) = bytes.split_at(str_len);
        *bytes = rest;
        str::from_utf8(s).map_err(DeserializeErrorKind::Utf8)
    }

    fn serialize_str(bytes: &mut Vec<u8>, s: &str) {
        leb128::write::unsigned(bytes, s.len() as u64).unwrap();
        bytes.extend_from_slice(s.as_bytes());
    }

    fn deserialize(bytes: &mut &[u8]) -> Result<Self, DeserializeError> {
        let tag_count = leb128::read::unsigned(bytes).map_err(DeserializeErrorKind::Leb128)?;
        let mut architecture = None;
        let mut hasher = None;
        let mut depth = None;
        let mut internal_node_depth = None;

        for _ in 0..tag_count {
            let key = Self::deserialize_str(bytes)?;
            let value = Self::deserialize_str(bytes)?;
            match key {
                "architecture" => architecture = Some(value.to_owned()),
                "hasher" => hasher = Some(value.to_owned()),
                "depth" => {
                    let parsed =
                        value
                            .parse::<u8>()
                            .map_err(|err| DeserializeErrorKind::MalformedTag {
                                name: "depth",
                                err: err.into(),
                            })?;
                    depth = Some(parsed);
                }
                "internal_node_depth" => {
                    let parsed =
                        value
                            .parse::<u8>()
                            .map_err(|err| DeserializeErrorKind::MalformedTag {
                                name: "internal_node_depth",
                                err: err.into(),
                            })?;
                    internal_node_depth = Some(parsed);
                }
                _ => return Err(DeserializeErrorKind::UnknownTag(key.to_owned()).into()),
            }
        }
        Ok(Self {
            architecture: architecture.ok_or(DeserializeErrorKind::MissingTag("architecture"))?,
            depth: depth.ok_or(DeserializeErrorKind::MissingTag("depth"))?,
            internal_node_depth: internal_node_depth
                .ok_or(DeserializeErrorKind::MissingTag("internal_node_depth"))?,
            hasher: hasher.ok_or(DeserializeErrorKind::MissingTag("hasher"))?,
        })
    }

    fn serialize(&self, buffer: &mut Vec<u8>) {
        let entry_count = 4; // custom tags aren't supported (yet?)
        leb128::write::unsigned(buffer, entry_count).unwrap();

        Self::serialize_str(buffer, "architecture");
        Self::serialize_str(buffer, &self.architecture);
        Self::serialize_str(buffer, "depth");
        Self::serialize_str(buffer, &self.depth.to_string());
        Self::serialize_str(buffer, "internal_node_depth");
        Self::serialize_str(buffer, &self.internal_node_depth.to_string());
        Self::serialize_str(buffer, "hasher");
        Self::serialize_str(buffer, &self.hasher);
    }
}

impl Manifest {
    pub(super) fn deserialize(mut bytes: &[u8]) -> Result<Self, DeserializeError> {
        let version_count =
            leb128::read::unsigned(&mut bytes).map_err(DeserializeErrorKind::Leb128)?;
        let tags = TreeTags::deserialize(&mut bytes)?;

        Ok(Self {
            version_count,
            tags,
        })
    }

    pub(super) fn serialize(&self, buffer: &mut Vec<u8>) {
        leb128::write::unsigned(buffer, self.version_count).unwrap();
        self.tags.serialize(buffer);
    }
}
