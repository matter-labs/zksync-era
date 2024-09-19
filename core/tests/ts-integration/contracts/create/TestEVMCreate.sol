// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

interface IContractDeployer {
    function evmCodeHash(address key) external returns (bytes32);

    function createEVM(
        bytes calldata _initCode
    ) external payable returns (address newAddress);

    function create2EVM(
        bytes32 _salt,
        bytes calldata _initCode
    ) external payable returns (address);
}

/// @notice An example of a system contract that be used for local testing.
/// @dev It is not used anywhere except for testing
contract TestEVMCreate {
    IContractDeployer deployer = IContractDeployer(address(0x8006));

    function create(bytes calldata _code) external {
        deployer.createEVM(_code);
    }

    function create2(bytes32 _salt, bytes calldata _code) external payable {
        deployer.create2EVM{value:msg.value}(_salt, _code);
    }
}
