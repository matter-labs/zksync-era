// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity >=0.8.0;

/**
 * @author Matter Labs
 * @dev Common utilities used in zkSync system contracts
 */
library Utils {
    function safeCastToU128(uint256 _x) internal pure returns (uint128) {
        require(_x <= type(uint128).max, "Overflow");

        return uint128(_x);
    }

    function safeCastToU32(uint256 _x) internal pure returns (uint32) {
        require(_x <= type(uint32).max, "Overflow");

        return uint32(_x);
    }

    function safeCastToU24(uint256 _x) internal pure returns (uint24) {
        require(_x <= type(uint24).max, "Overflow");

        return uint24(_x);
    }

    /// @return codeLength The bytecode length in bytes
    function bytecodeLenInBytes(bytes32 _bytecodeHash) internal pure returns (uint256 codeLength) {
        codeLength = bytecodeLenInWords(_bytecodeHash) << 5; // _bytecodeHash * 32
    }

    /// @return codeLengthInWords The bytecode length in machine words
    function bytecodeLenInWords(bytes32 _bytecodeHash) internal pure returns (uint256 codeLengthInWords) {
        unchecked {
            codeLengthInWords = uint256(uint8(_bytecodeHash[2])) * 256 + uint256(uint8(_bytecodeHash[3]));
        }
    }
}
