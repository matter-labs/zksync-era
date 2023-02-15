// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

import {BOOTLOADER_FORMAL_ADDRESS} from "./Constants.sol";

/**
 * @author Matter Labs
 * @notice Contract that stores some of the context variables, that may be either
 * block-scoped, tx-scoped or system-wide.
 */
contract SystemContext {
    modifier onlyBootloader() {
        require(msg.sender == BOOTLOADER_FORMAL_ADDRESS);
        _;
    }

    /// @notice The chainId of the network. It is set at the genesis.
    uint256 public chainId;

    /// @notice The `tx.origin` in the current transaction.
    /// @dev It is updated before each transaction by the bootloader
    address public origin;

    /// @notice The `tx.gasPrice` in the current transaction.
    /// @dev It is updated before each transaction by the bootloader
    uint256 public gasPrice;

    /// @notice The current block's gasLimit (gasLimit in Ethereum terms).
    /// @dev Currently set to some dummy value, it will be changed closer to mainnet.
    uint256 public blockGasLimit = (1 << 30);

    /// @notice The `block.coinbase` in the current transaction.
    /// @dev For the support of coinbase, we will the bootloader formal address for now
    address public coinbase = BOOTLOADER_FORMAL_ADDRESS;

    /// @notice Formal `block.difficulty` parameter.
    uint256 public difficulty = 2500000000000000;

    /// @notice the field for the temporary value of msize. It was added before the
    /// msize was supported natively the VM and currently we keep it for the storage layout
    /// consistency.
    uint256 public msize = (1 << 24);

    /// @notice The `block.basefee`.
    /// @dev It is currently a constant.
    uint256 public baseFee;

    /// @notice The coefficient with which the current block's number
    /// is stored in the current block info
    uint256 constant BLOCK_INFO_BLOCK_NUMBER_PART = 2 ** 128;

    /// @notice block.number and block.timestamp stored packed.
    /// @dev It is equal to 2^128 * block_number + block_timestamp.
    uint256 public currentBlockInfo;

    /// @notice The hashes of blocks.
    /// @dev It stores block hashes for all previous blocks.
    mapping(uint256 => bytes32) public blockHash;

    /// @notice Set the current tx origin.
    /// @param _newOrigin The new tx origin.
    function setTxOrigin(address _newOrigin) external onlyBootloader {
        origin = _newOrigin;
    }

    /// @notice Set the current tx origin.
    /// @param _gasPrice The new tx gasPrice.
    function setGasPrice(uint256 _gasPrice) external onlyBootloader {
        gasPrice = _gasPrice;
    }

    /// @notice The method that emulates `blockhash` opcode in EVM.
    /// @dev Just like the blockhash in the EVM, it returns bytes32(0), when
    /// when queried about hashes that are older than 256 blocks ago.
    function getBlockHashEVM(uint256 _block) external view returns (bytes32 hash) {
        if (block.number < _block || block.number - _block > 256) {
            hash = bytes32(0);
        } else {
            hash = blockHash[_block];
        }
    }

    /// @notice Returns the current blocks' number and timestamp.
    /// @return blockNumber and blockTimestamp tuple of the current block's number and the current block's timestamp
    function getBlockNumberAndTimestamp() public view returns (uint256 blockNumber, uint256 blockTimestamp) {
        uint256 blockInfo = currentBlockInfo;
        blockNumber = blockInfo / BLOCK_INFO_BLOCK_NUMBER_PART;
        blockTimestamp = blockInfo % BLOCK_INFO_BLOCK_NUMBER_PART;
    }

    /// @notice Returns the current block's number.
    /// @return blockNumber The current block's number.
    function getBlockNumber() public view returns (uint256 blockNumber) {
        (blockNumber, ) = getBlockNumberAndTimestamp();
    }

    /// @notice Returns the current block's timestamp.
    /// @return timestamp The current block's timestamp.
    function getBlockTimestamp() public view returns (uint256 timestamp) {
        (, timestamp) = getBlockNumberAndTimestamp();
    }
}
