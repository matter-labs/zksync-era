// // Built-in deps
// // External deps
// use crate::franklin_crypto::bellman::pairing::{
//     bn256::Bn256,
//     ff::{PrimeField, PrimeFieldRepr, ScalarEngine},
//     CurveAffine, Engine,
// };
// use zksync_basic_types::U256;
// // Workspace deps
//
// pub struct EthereumSerializer;
//
// impl EthereumSerializer {
//     pub fn serialize_g1(point: &<Bn256 as Engine>::G1Affine) -> (U256, U256) {
//         if point.is_zero() {
//             return (U256::zero(), U256::zero());
//         }
//         let uncompressed = point.into_uncompressed();
//
//         let uncompressed_slice = uncompressed.as_ref();
//
//         // bellman serializes points as big endian and in the form x, y
//         // ethereum expects the same order in memory
//         let x = U256::from_big_endian(&uncompressed_slice[0..32]);
//         let y = U256::from_big_endian(&uncompressed_slice[32..64]);
//
//         (x, y)
//     }
//
//     pub fn serialize_g2(point: &<Bn256 as Engine>::G2Affine) -> ((U256, U256), (U256, U256)) {
//         let uncompressed = point.into_uncompressed();
//
//         let uncompressed_slice = uncompressed.as_ref();
//
//         // bellman serializes points as big endian and in the form x1*u, x0, y1*u, y0
//         // ethereum expects the same order in memory
//         let x_1 = U256::from_big_endian(&uncompressed_slice[0..32]);
//         let x_0 = U256::from_big_endian(&uncompressed_slice[32..64]);
//         let y_1 = U256::from_big_endian(&uncompressed_slice[64..96]);
//         let y_0 = U256::from_big_endian(&uncompressed_slice[96..128]);
//
//         ((x_1, x_0), (y_1, y_0))
//     }
//
//     pub fn serialize_fe(field_element: &<Bn256 as ScalarEngine>::Fr) -> U256 {
//         let mut be_bytes = [0u8; 32];
//         field_element
//             .into_repr()
//             .write_be(&mut be_bytes[..])
//             .expect("get new root BE bytes");
//         U256::from_big_endian(&be_bytes[..])
//     }
// }
//
// pub struct BitConvert;
//
// impl BitConvert {
//     /// Converts a set of bits to a set of bytes in direct order.
//     #[allow(clippy::wrong_self_convention)]
//     pub fn into_bytes(bits: Vec<bool>) -> Vec<u8> {
//         assert_eq!(bits.len() % 8, 0);
//         let mut message_bytes: Vec<u8> = vec![];
//
//         let byte_chunks = bits.chunks(8);
//         for byte_chunk in byte_chunks {
//             let mut byte = 0u8;
//             for (i, bit) in byte_chunk.iter().enumerate() {
//                 if *bit {
//                     byte |= 1 << i;
//                 }
//             }
//             message_bytes.push(byte);
//         }
//
//         message_bytes
//     }
//
//     /// Converts a set of bits to a set of bytes in reverse order for each byte.
//     #[allow(clippy::wrong_self_convention)]
//     pub fn into_bytes_ordered(bits: Vec<bool>) -> Vec<u8> {
//         assert_eq!(bits.len() % 8, 0);
//         let mut message_bytes: Vec<u8> = vec![];
//
//         let byte_chunks = bits.chunks(8);
//         for byte_chunk in byte_chunks {
//             let mut byte = 0u8;
//             for (i, bit) in byte_chunk.iter().rev().enumerate() {
//                 if *bit {
//                     byte |= 1 << i;
//                 }
//             }
//             message_bytes.push(byte);
//         }
//
//         message_bytes
//     }
//
//     /// Converts a set of Big Endian bytes to a set of bits.
//     pub fn from_be_bytes(bytes: &[u8]) -> Vec<bool> {
//         let mut bits = vec![];
//         for byte in bytes {
//             let mut temp = *byte;
//             for _ in 0..8 {
//                 bits.push(temp & 0x80 == 0x80);
//                 temp <<= 1;
//             }
//         }
//         bits
//     }
// }
//
// #[cfg(test)]
// mod test {
//     use super::*;
//
//     #[test]
//     fn test_bits_conversions() {
//         let mut bits = vec![];
//
//         bits.extend(vec![true, false, false, true, true, false, true, false]);
//         bits.extend(vec![false, false, true, true, false, true, true, false]);
//         bits.extend(vec![false, false, false, false, false, false, false, true]);
//
//         let bytes = BitConvert::into_bytes(bits.clone());
//         assert_eq!(bytes, vec![89, 108, 128]);
//
//         let bytes = BitConvert::into_bytes_ordered(bits.clone());
//         assert_eq!(bytes, vec![154, 54, 1]);
//
//         assert_eq!(BitConvert::from_be_bytes(&[154, 54, 1]), bits);
//     }
// }
