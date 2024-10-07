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

interface IAccountCodeStorage {
    function getRawCodeHash(address _address) external view returns (bytes32);
}

interface IRecursiveContract {
    function recurse(uint _depth) external returns (uint);
}

uint constant EVM_INTERPRETER_STIPEND = 1 << 30;

/**
 * Mock EVM interpreter used in low-level tests.
 */
contract MockEvmInterpreter is IRecursiveContract {
    IAccountCodeStorage constant ACCOUNT_CODE_STORAGE_CONTRACT = IAccountCodeStorage(address(0x8002));
    address constant CODE_ORACLE_ADDR = address(0x8012);

    /// Set to `true` for testing logic sanity.
    bool isUserSpace;

    modifier validEvmEntry() {
        if (!isUserSpace) {
            require(gasleft() >= EVM_INTERPRETER_STIPEND, "no stipend");
            // Fetch bytecode for the executed contract.
            bytes32 bytecodeHash = ACCOUNT_CODE_STORAGE_CONTRACT.getRawCodeHash(address(this));
            require(bytecodeHash != bytes32(0), "called contract not deployed");
            uint bytecodeVersion = uint(bytecodeHash) >> 248;
            require(bytecodeVersion == 2, "non-EVM bytecode");

            // Check that members of the current address are well-defined.
            require(address(this).code.length != 0, "invalid code");
            require(address(this).codehash == bytecodeHash, "bytecode hash mismatch");
        }
        _;
    }

    function testPayment(uint _expectedValue, uint _expectedBalance) public payable validEvmEntry {
        require(msg.value == _expectedValue, "unexpected msg.value");
        require(address(this).balance == _expectedBalance, "unexpected balance");
    }

    IRecursiveContract recursionTarget;

    function recurse(uint _depth) public validEvmEntry returns (uint) {
        // FIXME: for now, a new stipend is provided for each call
        // require(gasleft() < 2 * EVM_INTERPRETER_STIPEND, "stipend provided multiple times");
        if (_depth <= 1) {
            return 1;
        } else {
            IRecursiveContract target = (address(recursionTarget) == address(0)) ? this : recursionTarget;
            return target.recurse(_depth - 1) * _depth;
        }
    }

    function testRecursion(uint _depth, uint _expectedValue) external validEvmEntry returns (uint) {
        require(recurse(_depth) == _expectedValue, "incorrect recursion");
    }

    function testExternalRecursion(uint _depth, uint _expectedValue) external validEvmEntry returns (uint) {
        recursionTarget = new NativeRecursiveContract();
        uint returnedValue = recurse(_depth);
        recursionTarget = this; // This won't work on revert, but for tests, it's good enough
        require(returnedValue == _expectedValue, "incorrect recursion");
    }

    fallback() external validEvmEntry {
        require(msg.data.length == 0, "unsupported call");
    }
}

contract NativeRecursiveContract is IRecursiveContract {
    IRecursiveContract caller;

    constructor() {
        caller = IRecursiveContract(msg.sender);
    }

    function recurse(uint _depth) external returns (uint) {
        // FIXME: for now, the EVM stipend does spill to native contracts
        // require(gasleft() < EVM_INTERPRETER_STIPEND, "stipend spilled to native contract");
        return (_depth <= 1) ? 1 : caller.recurse(_depth - 1) * _depth;
    }
}
