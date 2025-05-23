// SPDX-License-Identifier: MIT

pragma solidity ^0.8.24;

import {Initializable} from "@openzeppelin/contracts-v4/proxy/utils/Initializable.sol";
import {BeaconProxy} from "@openzeppelin/contracts-v4/proxy/beacon/BeaconProxy.sol";
import {UpgradeableBeacon} from "@openzeppelin/contracts-v4/proxy/beacon/UpgradeableBeacon.sol";

import {L2StandardERC20V25} from "./L2StandardERC20V25.sol";
import {AddressAliasHelper} from "l1-contracts/vendor/AddressAliasHelper.sol";
import {L2_DEPLOYER_SYSTEM_CONTRACT_ADDR} from "l1-contracts/common/L2ContractAddresses.sol";
import {L2ContractHelper, IContractDeployer} from "l1-contracts/common/libraries/L2ContractHelper.sol";
import {SystemContractsCaller} from "l1-contracts/common/libraries/SystemContractsCaller.sol";

import {ZeroAddress, EmptyBytes32, Unauthorized, AddressMismatch, AmountMustBeGreaterThanZero, DeployFailed} from "l1-contracts/common/L1ContractErrors.sol";

/// @author Matter Labs
/// @custom:security-contact security@matterlabs.dev
/// @notice The "default" bridge implementation for the ERC20 tokens. Note, that it does not
/// support any custom token logic, i.e. rebase tokens' functionality is not supported.
contract L2SharedBridgeV25 is Initializable {
    /// @dev The address of the L1 shared bridge counterpart.
    address public l1SharedBridge;

    /// @dev Contract that stores the implementation address for token.
    /// @dev For more details see https://docs.openzeppelin.com/contracts/3.x/api/proxy#UpgradeableBeacon.
    UpgradeableBeacon public l2TokenBeacon;

    /// @dev Bytecode hash of the proxy for tokens deployed by the bridge.
    bytes32 internal l2TokenProxyBytecodeHash;

    /// @dev A mapping l2 token address => l1 token address
    mapping(address l2TokenAddress => address l1TokenAddress) public l1TokenAddress;

    /// @dev The address of the legacy L1 erc20 bridge counterpart.
    /// This is non-zero only on Era, and should not be renamed for backward compatibility with the SDKs.
    address public l1Bridge;

    /// @dev Contract is expected to be used as proxy implementation.
    /// @dev Disable the initialization to prevent Parity hack.
    uint256 public immutable ERA_CHAIN_ID;

    constructor(uint256 _eraChainId) {
        ERA_CHAIN_ID = _eraChainId;
        _disableInitializers();
    }

    /// @notice Initializes the bridge contract for later use. Expected to be used in the proxy.
    /// @param _l1SharedBridge The address of the L1 Bridge contract.
    /// @param _l1Bridge The address of the legacy L1 Bridge contract.
    /// @param _l2TokenProxyBytecodeHash The bytecode hash of the proxy for tokens deployed by the bridge.
    /// @param _aliasedOwner The address of the governor contract.
    function initialize(
        address _l1SharedBridge,
        address _l1Bridge,
        bytes32 _l2TokenProxyBytecodeHash,
        address _aliasedOwner
    ) external reinitializer(2) {
        if (_l1SharedBridge == address(0)) {
            revert ZeroAddress();
        }

        if (_l2TokenProxyBytecodeHash == bytes32(0)) {
            revert EmptyBytes32();
        }

        if (_aliasedOwner == address(0)) {
            revert ZeroAddress();
        }

        l1SharedBridge = _l1SharedBridge;

        if (block.chainid != ERA_CHAIN_ID) {
            address l2StandardToken = address(new L2StandardERC20V25{salt: bytes32(0)}());
            l2TokenBeacon = new UpgradeableBeacon{salt: bytes32(0)}(l2StandardToken);
            l2TokenProxyBytecodeHash = _l2TokenProxyBytecodeHash;
            l2TokenBeacon.transferOwnership(_aliasedOwner);
        } else {
            if (_l1Bridge == address(0)) {
                revert ZeroAddress();
            }
            l1Bridge = _l1Bridge;
            // l2StandardToken and l2TokenBeacon are already deployed on ERA, and stored in the proxy
        }
    }

    /// @notice Finalize the deposit and mint funds
    /// @param _l1Sender The account address that initiated the deposit on L1
    /// @param _l2Receiver The account address that would receive minted ether
    /// @param _l1Token The address of the token that was locked on the L1
    /// @param _amount Total amount of tokens deposited from L1
    /// @param _data The additional data that user can pass with the deposit
    function finalizeDeposit(
        address _l1Sender,
        address _l2Receiver,
        address _l1Token,
        uint256 _amount,
        bytes calldata _data
    ) external {
        // Only the L1 bridge counterpart can initiate and finalize the deposit.
        if (
            AddressAliasHelper.undoL1ToL2Alias(msg.sender) != l1Bridge &&
            AddressAliasHelper.undoL1ToL2Alias(msg.sender) != l1SharedBridge
        ) {
            revert Unauthorized(msg.sender);
        }

        address expectedL2Token = l2TokenAddress(_l1Token);
        address currentL1Token = l1TokenAddress[expectedL2Token];
        if (currentL1Token == address(0)) {
            address deployedToken = _deployL2Token(_l1Token, _data);
            if (deployedToken != expectedL2Token) {
                revert AddressMismatch(expectedL2Token, deployedToken);
            }

            l1TokenAddress[expectedL2Token] = _l1Token;
        } else {
            if (currentL1Token != _l1Token) {
                revert AddressMismatch(_l1Token, currentL1Token);
            }
        }

        L2StandardERC20V25(expectedL2Token).bridgeMint(_l2Receiver, _amount);
    }

    /// @dev Deploy and initialize the L2 token for the L1 counterpart
    function _deployL2Token(address _l1Token, bytes calldata _data) internal returns (address) {
        bytes32 salt = _getCreate2Salt(_l1Token);

        BeaconProxy l2Token = _deployBeaconProxy(salt);
        L2StandardERC20V25(address(l2Token)).bridgeInitialize(_l1Token, _data);

        return address(l2Token);
    }

    /// @notice Initiates a withdrawal by burning funds on the contract and sending the message to L1
    /// where tokens would be unlocked
    /// @param _l1Receiver The account address that should receive funds on L1
    /// @param _l2Token The L2 token address which is withdrawn
    /// @param _amount The total amount of tokens to be withdrawn
    function withdraw(address _l1Receiver, address _l2Token, uint256 _amount) external {
        if (_amount == 0) {
            revert AmountMustBeGreaterThanZero();
        }

        L2StandardERC20V25(_l2Token).bridgeBurn(msg.sender, _amount);

        address l1Token = l1TokenAddress[_l2Token];
        if (l1Token == address(0)) {
            revert ZeroAddress();
        }

        bytes memory message = _getL1WithdrawMessage(_l1Receiver, l1Token, _amount);
        L2ContractHelper.sendMessageToL1(message);
    }

    /// @dev Encode the message for l2ToL1log sent with withdraw initialization
    function _getL1WithdrawMessage(address, address, uint256) internal pure returns (bytes memory) {
        // For the ease of porting the code for the testing purposes.
        revert("Unimplemented in this copied version");
    }

    /// @return Address of an L2 token counterpart
    function l2TokenAddress(address _l1Token) public view returns (address) {
        bytes32 constructorInputHash = keccak256(abi.encode(address(l2TokenBeacon), ""));
        bytes32 salt = _getCreate2Salt(_l1Token);
        return
            L2ContractHelper.computeCreate2Address(address(this), salt, l2TokenProxyBytecodeHash, constructorInputHash);
    }

    /// @dev Convert the L1 token address to the create2 salt of deployed L2 token
    function _getCreate2Salt(address _l1Token) internal pure returns (bytes32 salt) {
        salt = bytes32(uint256(uint160(_l1Token)));
    }

    /// @dev Deploy the beacon proxy for the L2 token, while using ContractDeployer system contract.
    /// @dev This function uses raw call to ContractDeployer to make sure that exactly `l2TokenProxyBytecodeHash` is used
    /// for the code of the proxy.
    function _deployBeaconProxy(bytes32 salt) internal returns (BeaconProxy proxy) {
        (bool success, bytes memory returndata) = SystemContractsCaller.systemCallWithReturndata(
            uint32(gasleft()),
            L2_DEPLOYER_SYSTEM_CONTRACT_ADDR,
            0,
            abi.encodeCall(
                IContractDeployer.create2,
                (salt, l2TokenProxyBytecodeHash, abi.encode(address(l2TokenBeacon), ""))
            )
        );

        // The deployment should be successful and return the address of the proxy
        if (!success) {
            revert DeployFailed();
        }
        proxy = BeaconProxy(abi.decode(returndata, (address)));
    }
}
