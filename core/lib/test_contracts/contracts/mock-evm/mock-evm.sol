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

    function publishEVMBytecode(uint256 _bytecodeLen, bytes calldata _bytecode) external {
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

    IAccountCodeStorage constant ACCOUNT_CODE_STORAGE_CONTRACT = IAccountCodeStorage(address(0x8002));
    MockKnownCodeStorage constant KNOWN_CODE_STORAGE_CONTRACT = MockKnownCodeStorage(address(0x8004));

    /// The returned value is obviously incorrect in the general case, but works well enough when called by the bootloader.
    function extendedAccountVersion(address _address) public view returns (AccountAbstractionVersion) {
        return AccountAbstractionVersion.Version1;
    }

    /// Replaces real deployment with publishing a surrogate EVM "bytecode".
    /// @param _salt bytecode hash
    /// @param _input bytecode to publish
    function create(
        bytes32 _salt,
        bytes32, // ignored, since it's not possible to set arbitrarily
        bytes calldata _input
    ) external payable returns (address) {
        KNOWN_CODE_STORAGE_CONTRACT.setEVMBytecodeHash(_salt);
        KNOWN_CODE_STORAGE_CONTRACT.publishEVMBytecode(_input.length, _input);
        address newAddress = address(uint160(msg.sender) + 1);
        ACCOUNT_CODE_STORAGE_CONTRACT.storeAccountConstructedCodeHash(newAddress, _salt);
        return newAddress;
    }

    bytes32 constant CREATE2_PREFIX = keccak256("zksyncCreate2");

    /// Mocks `create2` with real counterpart semantics, other than bytecode passed in `_input`.
    /// @param _input bytecode to publish
    function create2(
        bytes32 _salt,
        bytes32 _bytecodeHash,
        bytes calldata _input
    ) external payable returns (address newAddress) {
        KNOWN_CODE_STORAGE_CONTRACT.setEVMBytecodeHash(_bytecodeHash);
        KNOWN_CODE_STORAGE_CONTRACT.publishEVMBytecode(_input.length, _input);

        bytes32 hash = keccak256(
            bytes.concat(CREATE2_PREFIX, bytes32(uint256(uint160(msg.sender))), _salt, _bytecodeHash)
        );
        newAddress = address(uint160(uint256(hash)));
        ACCOUNT_CODE_STORAGE_CONTRACT.storeAccountConstructedCodeHash(newAddress, _bytecodeHash);
    }
}

interface IAccountCodeStorage {
    function getRawCodeHash(address _address) external view returns (bytes32);
    function storeAccountConstructedCodeHash(address _address, bytes32 _hash) external;
}

interface IRecursiveContract {
    function recurse(uint _depth) external returns (uint);
}

interface IRecursiveDeployment {
    struct EvmDeployment {
        bytes32 bytecodeHash;
        /// Has fixed length to enable array slicing.
        bytes32 bytecode;
    }

    function testRecursiveDeployment(EvmDeployment[] calldata _deployments) external;
}

/// Native incrementing library. Not actually a library to simplify deployment.
contract IncrementingContract {
    // Should not collide with other storage slots
    uint constant INCREMENTED_SLOT = 0x123;

    function getIncrementedValue() public view returns (uint _value) {
        assembly {
            _value := sload(INCREMENTED_SLOT)
        }
    }

    function increment(address _thisAddress, uint _thisBalance) external {
        require(msg.sender == tx.origin, "msg.sender not retained");
        require(address(this) == _thisAddress, "this address");
        require(address(this).balance == _thisBalance, "this balance");
        assembly {
            sstore(INCREMENTED_SLOT, add(sload(INCREMENTED_SLOT), 1))
        }
    }

    /// Tests delegation to a native or EVM contract at the specified target.
    function testDelegateCall(address _target) external {
        uint valueSnapshot = getIncrementedValue();
        (bool success, ) = _target.delegatecall(abi.encodeCall(
            IncrementingContract.increment,
            (address(this), address(this).balance)
        ));
        require(success, "delegatecall reverted");
        require(getIncrementedValue() == valueSnapshot + 1, "invalid value");
    }

    function testStaticCall(address _target, uint _expectedValue) external {
        (bool success, bytes memory rawValue) = _target.staticcall(abi.encodeCall(
            this.getIncrementedValue,
            ()
        ));
        require(success, "static call reverted");
        (uint value) = abi.decode(rawValue, (uint));
        require(value == _expectedValue, "value mismatch");

        (success, ) = _target.staticcall(abi.encodeCall(
            IncrementingContract.increment,
            (address(this), address(this).balance)
        ));
        require(!success, "staticcall should've reverted");
    }
}

uint constant EVM_EMULATOR_STIPEND = 1 << 30;

/**
 * Mock EVM emulator used in low-level tests.
 */
contract MockEvmEmulator is IRecursiveContract, IRecursiveDeployment, IncrementingContract {
    IAccountCodeStorage constant ACCOUNT_CODE_STORAGE_CONTRACT = IAccountCodeStorage(address(0x8002));

    /// Set to `true` for testing logic sanity.
    bool isUserSpace;

    modifier validEvmEntry() {
        if (!isUserSpace) {
            require(gasleft() >= EVM_EMULATOR_STIPEND, "no stipend");
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
        require(gasleft() < 2 * EVM_EMULATOR_STIPEND, "stipend provided multiple times");

        if (_depth <= 1) {
            return 1;
        } else {
            IRecursiveContract target = (address(recursionTarget) == address(0)) ? this : recursionTarget;
            // The real emulator limits amount of gas when performing far calls by EVM gas, so we emulate this behavior as well.
            uint gasToSend = isUserSpace ? gasleft() : (gasleft() - EVM_EMULATOR_STIPEND);
            return target.recurse{gas: gasToSend}(_depth - 1) * _depth;
        }
    }

    function testRecursion(uint _depth, uint _expectedValue) external validEvmEntry returns (uint) {
        require(recurse(_depth) == _expectedValue, "incorrect recursion");
    }

    function testExternalRecursion(uint _depth, uint _expectedValue) external validEvmEntry returns (uint) {
        recursionTarget = new NativeRecursiveContract(IRecursiveContract(this));
        uint returnedValue = recurse(_depth);
        recursionTarget = this; // This won't work on revert, but for tests, it's good enough
        require(returnedValue == _expectedValue, "incorrect recursion");
    }

    MockContractDeployer constant CONTRACT_DEPLOYER_CONTRACT = MockContractDeployer(address(0x8006));

    /// Emulates EVM contract deployment and a subsequent call to it in a single transaction.
    function testDeploymentAndCall(
        bytes32 _evmBytecodeHash,
        bytes calldata _evmBytecode,
        bool _revert
    ) external validEvmEntry {
        IRecursiveContract newContract = IRecursiveContract(CONTRACT_DEPLOYER_CONTRACT.create(
            _evmBytecodeHash,
            _evmBytecodeHash,
            _evmBytecode
        ));
        require(uint160(address(newContract)) == uint160(address(this)) + 1, "unexpected address");
        require(address(newContract).code.length > 0, "contract code length");
        require(address(newContract).codehash != bytes32(0), "contract code hash");

        uint gasToSend = gasleft() - EVM_EMULATOR_STIPEND;
        require(newContract.recurse{gas: gasToSend}(5) == 120, "unexpected recursive result");
        require(!_revert, "requested revert");
    }

    function testCallToPreviousDeployment() external validEvmEntry {
        IRecursiveContract newContract = IRecursiveContract(address(uint160(address(this)) + 1));
        require(address(newContract).code.length > 0, "contract code length");
        require(address(newContract).codehash != bytes32(0), "contract code hash");

        uint gasToSend = gasleft() - EVM_EMULATOR_STIPEND;
        require(newContract.recurse{gas: gasToSend}(5) == 120, "unexpected recursive result");
    }

    function testRecursiveDeployment(EvmDeployment[] calldata _deployments) external override validEvmEntry {
        if (_deployments.length == 0) {
            return;
        }

        IRecursiveDeployment newContract = IRecursiveDeployment(CONTRACT_DEPLOYER_CONTRACT.create(
            _deployments[0].bytecodeHash,
            _deployments[0].bytecodeHash,
            bytes.concat(_deployments[0].bytecode)
        ));
        uint gasToSend = gasleft() - EVM_EMULATOR_STIPEND;
        newContract.testRecursiveDeployment{gas: gasToSend}(_deployments[1:]);
    }

    function testDeploymentWithPartialRevert(
        EvmDeployment[] calldata _deployments,
        bool[] calldata _shouldRevert
    ) external validEvmEntry {
        require(_deployments.length == _shouldRevert.length, "length mismatch");

        for (uint i = 0; i < _deployments.length; i++) {
            uint gasToSend = gasleft() - EVM_EMULATOR_STIPEND;
            try this.deployThenRevert{gas: gasToSend}(
                _deployments[i],
                bytes32(i),
                _shouldRevert[i]
            ) returns(address newAddress) {
                require(!_shouldRevert[i], "unexpected deploy success");
                require(newAddress.code.length > 0, "contract code length");
                require(newAddress.codehash != bytes32(0), "contract code hash");
            } catch Error(string memory reason) {
                require(_shouldRevert[i], "unexpected revert");
                require(keccak256(bytes(reason)) == keccak256("requested revert"), "unexpected error");
            }
        }
    }

    function deployThenRevert(
        EvmDeployment calldata _deployment,
        bytes32 _salt,
        bool _shouldRevert
    ) external validEvmEntry returns (address newAddress) {
        newAddress = CONTRACT_DEPLOYER_CONTRACT.create2(
            _salt,
            _deployment.bytecodeHash,
            bytes.concat(_deployment.bytecode)
        );
        require(newAddress.code.length > 0, "contract code length");
        require(newAddress.codehash != bytes32(0), "contract code hash");

        require(!_shouldRevert, "requested revert");
    }

    fallback() external validEvmEntry {
        require(msg.data.length == 0, "unsupported call");
    }
}

contract NativeRecursiveContract is IRecursiveContract {
    IRecursiveContract target;

    constructor(IRecursiveContract _target) {
        target = _target;
    }

    function recurse(uint _depth) external returns (uint) {
        require(gasleft() < EVM_EMULATOR_STIPEND, "stipend spilled to native contract");
        return (_depth <= 1) ? 1 : target.recurse(_depth - 1) * _depth;
    }
}
