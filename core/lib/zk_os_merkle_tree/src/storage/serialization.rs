//! Binary serialization of tree nodes.

use zksync_basic_types::H256;

use crate::{
    errors::{DeserializeContext, DeserializeErrorKind},
    storage::InsertedKeyEntry,
    types::{ChildRef, InternalNode, Leaf, Manifest, Root},
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
        let prev_index =
            leb128::read::unsigned(&mut buffer).map_err(DeserializeErrorKind::Leb128)?;
        let next_index =
            leb128::read::unsigned(&mut buffer).map_err(DeserializeErrorKind::Leb128)?;
        if !buffer.is_empty() {
            return Err(DeserializeErrorKind::Leftovers.into());
        }
        Ok(Self {
            key,
            value,
            prev_index,
            next_index,
        })
    }

    pub(super) fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(self.key.as_bytes());
        buffer.extend_from_slice(self.value.as_bytes());
        leb128::write::unsigned(buffer, self.prev_index).unwrap();
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

impl Manifest {
    pub(super) fn deserialize(mut bytes: &[u8]) -> Result<Self, DeserializeError> {
        let version_count =
            leb128::read::unsigned(&mut bytes).map_err(DeserializeErrorKind::Leb128)?;

        Ok(Self { version_count })
    }

    pub(super) fn serialize(&self, buffer: &mut Vec<u8>) {
        leb128::write::unsigned(buffer, self.version_count).unwrap();
    }
}
