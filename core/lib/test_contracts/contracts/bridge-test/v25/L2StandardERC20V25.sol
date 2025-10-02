// SPDX-License-Identifier: MIT

pragma solidity ^0.8.24;

import {ERC20PermitUpgradeable} from "@openzeppelin/contracts-upgradeable-v4/token/ERC20/extensions/draft-ERC20PermitUpgradeable.sol";
import {UpgradeableBeacon} from "@openzeppelin/contracts-v4/proxy/beacon/UpgradeableBeacon.sol";
import {ERC1967Upgrade} from "@openzeppelin/contracts-v4/proxy/ERC1967/ERC1967Upgrade.sol";

import {ZeroAddress, Unauthorized, NonSequentialVersion} from "l1-contracts/common/L1ContractErrors.sol";

/// @author Matter Labs
/// @custom:security-contact security@matterlabs.dev
/// @notice The ERC20 token implementation, that is used in the "default" ERC20 bridge. Note, that it does not
/// support any custom token logic, i.e. rebase tokens' functionality is not supported.
contract L2StandardERC20V25 is ERC20PermitUpgradeable, ERC1967Upgrade {
    /// @dev Describes whether there is a specific getter in the token.
    /// @notice Used to explicitly separate which getters the token has and which it does not.
    /// @notice Different tokens in L1 can implement or not implement getter function as `name`/`symbol`/`decimals`,
    /// @notice Our goal is to store all the getters that L1 token implements, and for others, we keep it as an unimplemented method.
    struct ERC20Getters {
        bool ignoreName;
        bool ignoreSymbol;
        bool ignoreDecimals;
    }

    ERC20Getters private availableGetters;

    /// @dev The decimals of the token, that are used as a value for `decimals` getter function.
    /// @notice A private variable is used only for decimals, but not for `name` and `symbol`, because standard
    /// @notice OpenZeppelin token represents `name` and `symbol` as storage variables and `decimals` as constant.
    uint8 private decimals_;

    /// @dev Address of the L2 bridge that is used as trustee who can mint/burn tokens
    address public l2Bridge;

    /// @dev Address of the L1 token that can be deposited to mint this L2 token
    address public l1Address;

    /// @dev Contract is expected to be used as proxy implementation.
    constructor() {
        // Disable initialization to prevent Parity hack.
        _disableInitializers();
    }

    /// @notice Initializes a contract token for later use. Expected to be used in the proxy.
    /// @dev Stores the L1 address of the bridge and set `name`/`symbol`/`decimals` getters that L1 token has.
    /// @param _l1Address Address of the L1 token that can be deposited to mint this L2 token
    /// @param _data The additional data that the L1 bridge provide for initialization.
    /// In this case, it is packed `name`/`symbol`/`decimals` of the L1 token.
    function bridgeInitialize(address _l1Address, bytes calldata _data) external initializer {
        if (_l1Address == address(0)) {
            revert ZeroAddress();
        }
        l1Address = _l1Address;

        l2Bridge = msg.sender;

        // We parse the data exactly as they were created on the L1 bridge
        (bytes memory nameBytes, bytes memory symbolBytes, bytes memory decimalsBytes) = abi.decode(
            _data,
            (bytes, bytes, bytes)
        );

        ERC20Getters memory getters;
        string memory decodedName;
        string memory decodedSymbol;

        // L1 bridge didn't check if the L1 token return values with proper types for `name`/`symbol`/`decimals`
        // That's why we need to try to decode them, and if it works out, set the values as getters.

        // NOTE: Solidity doesn't have a convenient way to try to decode a value:
        // - Decode them manually, i.e. write a function that will validate that data in the correct format
        // and return decoded value and a boolean value - whether it was possible to decode.
        // - Use the standard abi.decode method, but wrap it into an external call in which error can be handled.
        // We use the second option here.

        try this.decodeString(nameBytes) returns (string memory nameString) {
            decodedName = nameString;
        } catch {
            getters.ignoreName = true;
        }

        try this.decodeString(symbolBytes) returns (string memory symbolString) {
            decodedSymbol = symbolString;
        } catch {
            getters.ignoreSymbol = true;
        }

        // Set decoded values for name and symbol.
        __ERC20_init_unchained(decodedName, decodedSymbol);

        // Set the name for EIP-712 signature.
        __ERC20Permit_init(decodedName);

        try this.decodeUint8(decimalsBytes) returns (uint8 decimalsUint8) {
            // Set decoded value for decimals.
            decimals_ = decimalsUint8;
        } catch {
            getters.ignoreDecimals = true;
        }

        availableGetters = getters;
    }

    /// @notice A method to be called by the governor to update the token's metadata.
    /// @param _availableGetters The getters that the token has.
    /// @param _newName The new name of the token.
    /// @param _newSymbol The new symbol of the token.
    /// @param _version The version of the token that will be initialized.
    /// @dev The _version must be exactly the version higher by 1 than the current version. This is needed
    /// to ensure that the governor can not accidentally disable future reinitialization of the token.
    function reinitializeToken(
        ERC20Getters calldata _availableGetters,
        string calldata _newName,
        string calldata _newSymbol,
        uint8 _version
    ) external onlyNextVersion(_version) reinitializer(_version) {
        // It is expected that this token is deployed as a beacon proxy, so we'll
        // allow the governor of the beacon to reinitialize the token.
        address beaconAddress = _getBeacon();
        if (msg.sender != UpgradeableBeacon(beaconAddress).owner()) {
            revert Unauthorized(msg.sender);
        }

        __ERC20_init_unchained(_newName, _newSymbol);
        __ERC20Permit_init(_newName);
        availableGetters = _availableGetters;
    }

    modifier onlyBridge() {
        if (msg.sender != l2Bridge) {
            revert Unauthorized(msg.sender);
        }
        _;
    }

    modifier onlyNextVersion(uint8 _version) {
        // The version should be incremented by 1. Otherwise, the governor risks disabling
        // future reinitialization of the token by providing too large a version.
        if (_version != _getInitializedVersion() + 1) {
            revert NonSequentialVersion();
        }
        _;
    }

    /// @dev Mint tokens to a given account.
    /// @param _to The account that will receive the created tokens.
    /// @param _amount The amount that will be created.
    /// @notice Should be called by bridge after depositing tokens from L1.
    function bridgeMint(address _to, uint256 _amount) external onlyBridge {
        _mint(_to, _amount);
    }

    /// @dev Burn tokens from a given account.
    /// @param _from The account from which tokens will be burned.
    /// @param _amount The amount that will be burned.
    /// @notice Should be called by bridge before withdrawing tokens to L1.
    function bridgeBurn(address _from, uint256 _amount) external onlyBridge {
        _burn(_from, _amount);
    }

    function name() public view override returns (string memory) {
        // If method is not available, behave like a token that does not implement this method - revert on call.
        // solhint-disable-next-line reason-string, gas-custom-errors
        if (availableGetters.ignoreName) revert();
        return super.name();
    }

    function symbol() public view override returns (string memory) {
        // If method is not available, behave like a token that does not implement this method - revert on call.
        // solhint-disable-next-line reason-string, gas-custom-errors
        if (availableGetters.ignoreSymbol) revert();
        return super.symbol();
    }

    function decimals() public view override returns (uint8) {
        // If method is not available, behave like a token that does not implement this method - revert on call.
        // solhint-disable-next-line reason-string, gas-custom-errors
        if (availableGetters.ignoreDecimals) revert();
        return decimals_;
    }

    /// @dev External function to decode a string from bytes.
    function decodeString(bytes calldata _input) external pure returns (string memory result) {
        (result) = abi.decode(_input, (string));
    }

    /// @dev External function to decode a uint8 from bytes.
    function decodeUint8(bytes calldata _input) external pure returns (uint8 result) {
        (result) = abi.decode(_input, (uint8));
    }
}
