// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

interface IContractDeployer {
    /// @notice Defines the version of the account abstraction protocol
    /// that a contract claims to follow.
    /// - `None` means that the account is just a contract and it should never be interacted
    /// with as a custom account
    /// - `Version1` means that the account follows the first version of the account abstraction protocol
    enum AccountAbstractionVersion {
        None,
        Version1
    }

    /// @notice Defines the nonce ordering used by the account
    /// - `Sequential` means that it is expected that the nonces are monotonic and increment by 1
    /// at a time (the same as EOAs).
    /// - `Arbitrary` means that the nonces for the accounts can be arbitrary. The operator
    /// should serve the transactions from such an account on a first-come-first-serve basis.
    /// @dev This ordering is more of a suggestion to the operator on how the AA expects its transactions
    /// to be processed and is not considered as a system invariant.
    enum AccountNonceOrdering {
        Sequential,
        Arbitrary
    }

    struct AccountInfo {
        AccountAbstractionVersion supportedAAVersion;
        AccountNonceOrdering nonceOrdering;
    }

    event ContractDeployed(
        address indexed deployerAddress,
        bytes32 indexed bytecodeHash,
        address indexed contractAddress
    );

    event AccountNonceOrderingUpdated(address indexed accountAddress, AccountNonceOrdering nonceOrdering);

    event AccountVersionUpdated(address indexed accountAddress, AccountAbstractionVersion aaVersion);

    event EVMProxyHashUpdated(bytes32 indexed oldHash, bytes32 indexed newHash);

    function getNewAddressCreate2(
        address _sender,
        bytes32 _bytecodeHash,
        bytes32 _salt,
        bytes calldata _input
    ) external view returns (address newAddress);

    function getNewAddressCreate(address _sender, uint256 _senderNonce) external pure returns (address newAddress);

    function getNewAddressCreate2EVM(
        address _sender,
        bytes32 _salt,
        bytes calldata _bytecode
    ) external view returns (address newAddress);

    function getNewAddressCreateEVM(address _sender, uint256 _senderNonce) external pure returns (address newAddress);

    function create2(
        bytes32 _salt,
        bytes32 _bytecodeHash,
        bytes calldata _input
    ) external payable returns (address newAddress);

    function create2Account(
        bytes32 _salt,
        bytes32 _bytecodeHash,
        bytes calldata _input,
        AccountAbstractionVersion _aaVersion
    ) external payable returns (address newAddress);

    /// @dev While the `_salt` parameter is not used anywhere here,
    /// it is still needed for consistency between `create` and
    /// `create2` functions (required by the compiler).
    function create(
        bytes32 _salt,
        bytes32 _bytecodeHash,
        bytes calldata _input
    ) external payable returns (address newAddress);

    /// @dev While `_salt` is never used here, we leave it here as a parameter
    /// for the consistency with the `create` function.
    function createAccount(
        bytes32 _salt,
        bytes32 _bytecodeHash,
        bytes calldata _input,
        AccountAbstractionVersion _aaVersion
    ) external payable returns (address newAddress);

    /// @notice Returns the information about a certain AA.
    function getAccountInfo(address _address) external view returns (AccountInfo memory info);

    /// @notice Can be called by an account to update its account version
    function updateAccountVersion(AccountAbstractionVersion _version) external;

    /// @notice Can be called by an account to update its nonce ordering
    function updateNonceOrdering(AccountNonceOrdering _nonceOrdering) external;

    /// @notice code hash of an evm contract
    // function getCodeHash(address _addr) external view returns (bytes32);

    /// @notice size of an evm contracts code
    // function getCodeSize(address addr) external view returns (uint256);

    /// @notice code for a given evm contract
    // function getCode(address addr) external view returns (bytes memory code);

    /// @notice prepares needed data for evm execution
    // function prepareEvmExecution(address codeAddress) external returns (bool isConstructor, bytes memory bytecode);

    function createEVM(bytes calldata _initCode) external payable returns (address newAddress);

    function create2EVM(bytes32 _salt, bytes calldata _initCode) external payable returns (address);

    function evmCode(address) external view returns (bytes memory);
    function evmCodeHash(address) external view returns (bytes32);

    // TODO: this is a hack before rewriting to assembly.
    // This is the only reliable way to pass gas into constructor
    // function constructorGas(address) external view returns (uint256);

    function setDeployedCode(uint256 constructorGasLeft, bytes calldata newDeployedCode) external;

    function constructorReturnGas() external view returns (uint256);
}
