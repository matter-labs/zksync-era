// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

/**
 * Mock `KnownCodeStorage` counterpart producing `MarkedAsKnown` events and having `publishEVMBytecode` method
 * added for EVM emulation, calls to which should be traced by the host.
 */
contract MockKnownCodeStorage {
    event MarkedAsKnown(bytes32 indexed bytecodeHash, bool indexed sendBytecodeToL1);

    function markFactoryDeps(bool _shouldSendToL1, bytes32[] calldata _hashes) external {
        unchecked {
            uint256 hashesLen = _hashes.length;
            for (uint256 i = 0; i < hashesLen; ++i) {
                _markBytecodeAsPublished(_hashes[i], _shouldSendToL1);
            }
        }
    }

    function markBytecodeAsPublished(bytes32 _bytecodeHash) external {
        _markBytecodeAsPublished(_bytecodeHash, false);
    }

    function _markBytecodeAsPublished(bytes32 _bytecodeHash, bool _shouldSendToL1) internal {
        if (getMarker(_bytecodeHash) == 0) {
            assembly {
                sstore(_bytecodeHash, 1)
            }
            emit MarkedAsKnown(_bytecodeHash, _shouldSendToL1);
        }
    }

    bytes32 evmBytecodeHash; // For tests, it's OK to potentially collide with the marker slot for hash `bytes32(0)`

    /// Sets the EVM bytecode hash to be used in the next `publishEVMBytecode` call.
    function setEVMBytecodeHash(bytes32 _bytecodeHash) external {
        evmBytecodeHash = _bytecodeHash;
    }

    function publishEVMBytecode(bytes calldata _bytecode) external {
        bytes32 hash = evmBytecodeHash;
        require(hash != bytes32(0), "EVM bytecode hash not set");

        if (getMarker(evmBytecodeHash) == 0) {
            assembly {
                sstore(hash, 1)
            }
        }
        emit MarkedAsKnown(hash, getMarker(hash) == 0);
        evmBytecodeHash = bytes32(0);
    }

    function getMarker(bytes32 _hash) public view returns (uint256 marker) {
        assembly {
            marker := sload(_hash)
        }
    }
}

/**
 * Mock `ContractDeployer` counterpart focusing on EVM bytecode deployment (via `create`; this isn't how real EVM bytecode deployment works,
 * but it's good enough for low-level tests).
 */
contract MockContractDeployer {
    enum AccountAbstractionVersion {
        None,
        Version1
    }

    address constant CODE_ORACLE_ADDR = address(0x8012);
    MockKnownCodeStorage constant KNOWN_CODE_STORAGE_CONTRACT = MockKnownCodeStorage(address(0x8004));

    /// The returned value is obviously incorrect in the general case, but works well enough when called by the bootloader.
    function extendedAccountVersion(address _address) public view returns (AccountAbstractionVersion) {
        return AccountAbstractionVersion.Version1;
    }

    /// Replaces real deployment with publishing a surrogate EVM "bytecode".
    /// @param _salt bytecode hash
    /// @param _bytecodeHash ignored, since it's not possible to set arbitrarily
    /// @param _input bytecode to publish
    function create(
        bytes32 _salt,
        bytes32 _bytecodeHash,
        bytes calldata _input
    ) external payable returns (address) {
        KNOWN_CODE_STORAGE_CONTRACT.setEVMBytecodeHash(_salt);
        KNOWN_CODE_STORAGE_CONTRACT.publishEVMBytecode(_input);
        return address(0);
    }
}
