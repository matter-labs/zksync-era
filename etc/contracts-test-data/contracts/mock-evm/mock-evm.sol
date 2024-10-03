// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract MockKnownCodeStorage {
    function markFactoryDeps(bool _shouldSendToL1, bytes32[] calldata _hashes) external {
        // Do nothing; necessary for system contracts to function correctly
    }

    function markBytecodeAsPublished(bytes32 _bytecodeHash) external {
        // Do nothing; necessary for system contracts to function correctly
    }

    function publishEVMBytecode(bytes calldata _bytecode) external {
        // Do nothing; necessary for system contracts to function correctly
    }

    function getMarker(bytes32 _hash) public view returns (uint256 marker) {
        assembly {
            marker := sload(_hash)
        }
    }
}

contract MockContractDeployer {
    enum AccountAbstractionVersion {
        None,
        Version1
    }

    address constant CODE_ORACLE_ADDR = address(0x8012);
    MockKnownCodeStorage constant KNOWN_CODE_STORAGE_CONTRACT = MockKnownCodeStorage(address(0x8004));

    function extendedAccountVersion(address _address) public view returns (AccountAbstractionVersion) {
        // The returned value is obviously false in the general case, but works well enough in the bootloader context
        return AccountAbstractionVersion.Version1;
    }

    // Replaces the real deployment by publishing a surrogate EVM "bytecode" obtained by concatenating `_salt` and `_input`.
    // The call to publish this bytecode should be intercepted by the tracer.
    function create(
        bytes32 _salt,
        bytes32 _bytecodeHash,
        bytes calldata _input
    ) external payable returns (address) {
        bytes memory evmBytecode = bytes.concat(_salt, _input);
        KNOWN_CODE_STORAGE_CONTRACT.publishEVMBytecode(evmBytecode);
        return address(0);
    }
}
