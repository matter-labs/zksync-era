// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

library RLPEncoder {
	function encodeAddress(address _val) internal pure returns (bytes memory encoded) {
		// The size is equal to 14 bytes of the address itself + 1 for encoding bytes length in RLP.
		encoded = new bytes(0x15);

		bytes20 shiftedVal = bytes20(_val);
		assembly {
			// In the first byte we write the encoded length as 0x80 + 0x14 == 0x94.
			mstore(add(encoded, 0x20), 0x9400000000000000000000000000000000000000000000000000000000000000)
			// Write address data without stripping zeros.
			mstore(add(encoded, 0x21), shiftedVal)
		}
	}

	function encodeUint256(uint256 _val) internal pure returns (bytes memory encoded) {
		unchecked {
			if (_val < 128) {
				encoded = new bytes(1);
				// Handle zero as a non-value, since stripping zeroes results in an empty byte array
				encoded[0] = (_val == 0) ? bytes1(uint8(128)) : bytes1(uint8(_val));
			} else {
				uint256 hbs = _highestByteSet(_val);

				encoded = new bytes(hbs + 2);
				encoded[0] = bytes1(uint8(hbs + 0x81));

				uint256 lbs = 31 - hbs;
				uint256 shiftedVal = _val << (lbs * 8);

				assembly {
					mstore(add(encoded, 0x21), shiftedVal)
				}
			}
		}
	}

	/// @notice Encodes the size of bytes in RLP format.
	/// NOTE: panics if the length is 1, since the length encoding is ambiguous in this case.
	function encodeNonSingleBytesLen(uint256 _len) internal pure returns (bytes memory) {
		assert(_len != 1);
		return _encodeLength(_len, 0x80);
	}

	/// @notice Encodes the size of list items in RLP format.
	function encodeListLen(uint256 _len) internal pure returns (bytes memory) {
		return _encodeLength(_len, 0xc0);
	}

	function _encodeLength(uint256 _len, uint256 _offset) private pure returns (bytes memory encoded) {
		unchecked {
			if (_len < 56) {
				encoded = new bytes(1);
				encoded[0] = bytes1(uint8(_len + _offset));
			} else {
				uint256 hbs = _highestByteSet(_len);

				encoded = new bytes(hbs + 2);
				encoded[0] = bytes1(uint8(_offset + hbs + 56));

				uint256 lbs = 31 - hbs;
				uint256 shiftedVal = _len << (lbs * 8);

				assembly {
					mstore(add(encoded, 0x21), shiftedVal)
				}
			}
		}
	}

	/// @notice Computes the index of the highest byte set in number.
	/// @notice Uses little endian ordering (The least significant byte has index `0`).
	/// NOTE: returns `0` for `0`
	function _highestByteSet(uint256 _number) private pure returns (uint256 hbs) {
		// TODO: for optimization, the comparison can be replaced with bitwise operations
		// should be resolver after evaluating the cost of opcodes.
		if (_number >= 2**128) {
			_number >>= 128;
			hbs += 16;
		}
		if (_number >= 2**64) {
			_number >>= 64;
			hbs += 8;
		}
		if (_number >= 2**32) {
			_number >>= 32;
			hbs += 4;
		}
		if (_number >= 2**16) {
			_number >>= 16;
			hbs += 2;
		}
		if (_number >= 2**8) {
			hbs += 1;
		}
	}
}
