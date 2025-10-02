use zksync_basic_types::U256;

// Starting with version 1 for this compression strategy. Any modifications to our current strategy MUST
// increment this number.
pub const COMPRESSION_VERSION_NUMBER: u8 = 1;

// Trait used to define functionality for different compression modes. Defines functions for
// output size, what type of operation was performed, and value/extended compression.
trait CompressionMode: 'static {
    /// Id of the operation being performed.
    fn operation_id(&self) -> usize;
    /// Gets the diff and size of value
    fn get_diff_and_size(&self) -> Option<(U256, usize)>;
    /// Number of bytes the compressed value requires. None indicates that compression cannot be performed for the
    /// given strategy.
    fn output_size(&self) -> Option<usize> {
        self.get_diff_and_size().map(|(_, size)| size)
    }
    /// Compress the value.
    fn compress_value_only(&self) -> Option<Vec<u8>> {
        let (diff, size) = self.get_diff_and_size()?;

        let mut buffer = [0u8; 32];
        diff.to_big_endian(&mut buffer);

        let diff = buffer[(32 - size)..].to_vec();

        Some(diff)
    }
    /// Concatenation of the metadata byte (5 bits for len and 3 bits for operation type) and the compressed value.
    fn compress_extended(&self) -> Option<Vec<u8>> {
        self.compress_value_only().map(|compressed_value| {
            let mut res: Vec<u8> = vec![];
            res.push(metadata_byte(
                self.output_size().unwrap(),
                self.operation_id(),
            ));
            res.extend(compressed_value);
            res
        })
    }
}

struct CompressionByteAdd {
    pub prev_value: U256,
    pub new_value: U256,
}

impl CompressionMode for CompressionByteAdd {
    fn operation_id(&self) -> usize {
        1
    }

    fn get_diff_and_size(&self) -> Option<(U256, usize)> {
        let diff = self.new_value.overflowing_sub(self.prev_value).0;
        // Ceiling division
        let size = diff.bits().div_ceil(8);

        if size >= 31 {
            None
        } else {
            Some((diff, size))
        }
    }
}

struct CompressionByteSub {
    pub prev_value: U256,
    pub new_value: U256,
}

impl CompressionMode for CompressionByteSub {
    fn operation_id(&self) -> usize {
        2
    }

    fn get_diff_and_size(&self) -> Option<(U256, usize)> {
        let diff = self.prev_value.overflowing_sub(self.new_value).0;
        // Ceiling division
        let size = diff.bits().div_ceil(8);

        if size >= 31 {
            None
        } else {
            Some((diff, size))
        }
    }
}

struct CompressionByteTransform {
    pub new_value: U256,
}

impl CompressionMode for CompressionByteTransform {
    fn operation_id(&self) -> usize {
        3
    }

    fn get_diff_and_size(&self) -> Option<(U256, usize)> {
        // Ceiling division
        let size = self.new_value.bits().div_ceil(8);

        if size >= 31 {
            None
        } else {
            Some((self.new_value, size))
        }
    }
}

struct CompressionByteNone {
    pub new_value: U256,
}

impl CompressionByteNone {
    fn new(new_value: U256) -> Self {
        Self { new_value }
    }
}

impl CompressionMode for CompressionByteNone {
    fn operation_id(&self) -> usize {
        0
    }

    fn get_diff_and_size(&self) -> Option<(U256, usize)> {
        None
    }

    fn output_size(&self) -> Option<usize> {
        Some(32)
    }

    fn compress_value_only(&self) -> Option<Vec<u8>> {
        let mut buffer = [0u8; 32];
        self.new_value.to_big_endian(&mut buffer);

        Some(buffer.to_vec())
    }

    fn compress_extended(&self) -> Option<Vec<u8>> {
        let mut res = [0u8; 33];

        self.new_value.to_big_endian(&mut res[1..33]);
        Some(res.to_vec())
    }
}

fn default_passes(prev_value: U256, new_value: U256) -> Vec<Box<dyn CompressionMode>> {
    vec![
        Box::new(CompressionByteAdd {
            prev_value,
            new_value,
        }),
        Box::new(CompressionByteSub {
            prev_value,
            new_value,
        }),
        Box::new(CompressionByteTransform { new_value }),
    ]
}

/// Generates the metadata byte for a given compression strategy.
/// The metadata byte is structured as:
/// First 5 bits: length of the compressed value
/// Last 3 bits: operation id corresponding to the given compression used.
fn metadata_byte(output_size: usize, operation_id: usize) -> u8 {
    ((output_size << 3) | operation_id) as u8
}

/// Compresses storage values using the most efficient compression strategy.
///
/// For a given previous value and new value, tries each compression strategy selecting the most
/// efficient one. Using that strategy, generates the extended compression (metadata byte and compressed value).
/// If none are found then uses the full 32 byte new value with the metadata byte being `0x00`.
pub fn compress_with_best_strategy(prev_value: U256, new_value: U256) -> Vec<u8> {
    let compressors = default_passes(prev_value, new_value);

    compressors
        .iter()
        .filter_map(|e| e.compress_extended())
        .min_by_key(|bytes| bytes.len())
        .unwrap_or_else(|| {
            CompressionByteNone::new(new_value)
                .compress_extended()
                .unwrap()
        })
}

#[cfg(test)]
mod tests {
    use std::ops::{Add, BitAnd, Shr, Sub};

    use super::*;

    #[test]
    fn test_compress_addition() {
        let initial_val = U256::from(255438218);
        let final_val = U256::from(255438638);

        let compress_add_strategy = CompressionByteAdd {
            prev_value: initial_val,
            new_value: final_val,
        };

        verify_sub_is_none(initial_val, final_val);

        assert!(compress_add_strategy.output_size() == Some(2));

        assert!(compress_add_strategy.compress_value_only() == Some(vec![1, 164]));

        let compressed_val = compress_with_best_strategy(initial_val, final_val);

        assert!(compressed_val == vec![17, 1, 164]);

        let (metadata, compressed_val) = compressed_val.split_at(1);

        let metadata = U256::from(metadata);
        let operation = metadata.bitand(U256::from(7u8));
        assert!(operation == U256::from(compress_add_strategy.operation_id()));
        let len = metadata.shr(U256::from(3u8));
        assert!(len == U256::from(2));

        let compressed_val = U256::from(compressed_val);
        assert!(
            (((compressed_val.bits() as f64) / 8f64).ceil() as usize) == (len.as_u128() as usize)
        );
        assert!(initial_val.add(compressed_val) == final_val);
    }

    #[test]
    fn test_compress_subtraction() {
        let initial_val = U256::from(580481589);
        let final_val = U256::from(229496100);

        let compression_sub_strategy = CompressionByteSub {
            prev_value: initial_val,
            new_value: final_val,
        };

        verify_add_is_none(initial_val, final_val);

        assert!(compression_sub_strategy.output_size() == Some(4));

        assert!(compression_sub_strategy.compress_value_only() == Some(vec![20, 235, 157, 17]));
        assert!(compression_sub_strategy.compress_extended() == Some(vec![34, 20, 235, 157, 17]));

        let compressed_value = compress_with_best_strategy(initial_val, final_val);

        assert!(compressed_value == vec![34, 20, 235, 157, 17]);

        let (metadata, compressed_val) = compressed_value.split_at(1);

        let metadata = U256::from(metadata);
        let operation = metadata.bitand(U256::from(7u8));
        assert!(operation == U256::from(compression_sub_strategy.operation_id()));
        let len = metadata.shr(U256::from(3u8));
        assert!(len == U256::from(4));

        let compressed_val = U256::from(compressed_val);
        assert!(
            (((compressed_val.bits() as f64) / 8f64).ceil() as usize) == (len.as_u128() as usize)
        );
        assert!(initial_val.sub(compressed_val) == final_val);
    }

    #[test]
    fn test_compress_transform() {
        let initial_val = U256::from(580481589);
        let final_val = U256::from(1337);

        let compressed_value = compress_with_best_strategy(initial_val, final_val);
        assert!(compressed_value == vec![19, 5, 57]);

        let (metadata, compressed_val) = compressed_value.split_at(1);

        let metadata = U256::from(metadata);
        let operation = metadata.bitand(U256::from(7u8));
        assert!(operation == U256::from(3));
        let len = metadata.shr(U256::from(3u8));
        assert!(len == U256::from(2));

        let compressed_val = U256::from(compressed_val);
        assert!(
            (((compressed_val.bits() as f64) / 8f64).ceil() as usize) == (len.as_u128() as usize)
        );
        assert!(compressed_val == final_val);
    }

    #[test]
    fn test_compress_transform_to_zero() {
        let initial_val = U256::from(580481589);
        let final_val = U256::from(0);

        let compressed_value = compress_with_best_strategy(initial_val, final_val);
        assert!(compressed_value == vec![3]);

        let (metadata, compressed_val) = compressed_value.split_at(1);

        let metadata = U256::from(metadata);
        let operation = metadata.bitand(U256::from(7u8));
        assert!(operation == U256::from(3));
        let len = metadata.shr(U256::from(3u8));
        assert!(len == U256::from(0));

        let compressed_val = U256::from(compressed_val);
        assert!(
            (((compressed_val.bits() as f64) / 8f64).ceil() as usize) == (len.as_u128() as usize)
        );
        assert!(compressed_val == final_val);
    }

    #[test]
    fn test_compress_transform_to_one_from_max() {
        let initial_val = U256::MAX;
        let final_val = U256::from(1);

        let compressed_value = compress_with_best_strategy(initial_val, final_val);
        assert!(compressed_value == vec![9, 2]);

        let (metadata, compressed_val) = compressed_value.split_at(1);

        let metadata = U256::from(metadata);
        let operation = metadata.bitand(U256::from(7u8));
        assert!(operation == U256::from(1));
        let len = metadata.shr(U256::from(3u8));
        assert!(len == U256::from(1));

        let compressed_val = U256::from(compressed_val);
        assert!(
            (((compressed_val.bits() as f64) / 8f64).ceil() as usize) == (len.as_u128() as usize)
        );
    }

    #[test]
    fn test_compress_transform_to_u256_max() {
        let initial_val = U256::from(0);
        let final_val = U256::MAX;

        let compressed_value = compress_with_best_strategy(initial_val, final_val);
        assert!(compressed_value == vec![10, 1]);

        let (metadata, compressed_val) = compressed_value.split_at(1);

        let metadata = U256::from(metadata);
        let operation = metadata.bitand(U256::from(7u8));
        assert!(operation == U256::from(2));
        let len = metadata.shr(U256::from(3u8));
        assert!(len == U256::from(1));

        let compressed_val = U256::from(compressed_val);
        assert!((((compressed_val.bits() as f64) / 8f64).ceil() as usize) == 1);
    }

    fn verify_add_is_none(initial_val: U256, final_val: U256) {
        let compression_add_strategy = CompressionByteAdd {
            prev_value: initial_val,
            new_value: final_val,
        };

        assert!(compression_add_strategy.compress_value_only().is_none());
        assert!(compression_add_strategy.compress_extended().is_none());
    }

    fn verify_sub_is_none(initial_val: U256, final_val: U256) {
        let compression_sub_strategy = CompressionByteSub {
            prev_value: initial_val,
            new_value: final_val,
        };

        assert!(compression_sub_strategy.compress_value_only().is_none());
        assert!(compression_sub_strategy.compress_extended().is_none());
    }
}
