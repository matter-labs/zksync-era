// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

import "./Constants.sol";

/**
 * @author Matter Labs
 * @notice Contract that stores some of the context variables, that may be either 
 * block-scoped, tx-scoped or system-wide.
 */
contract SystemContext {
    modifier onlyBootloader {
        require(msg.sender == BOOTLOADER_FORMAL_ADDRESS);
        _;
    }
    
    uint256 public chainId = 270;
    address public origin;
    uint256 public gasPrice;
    // Some dummy value, maybe will be possible to change it in the future.
    uint256 public blockGasLimit = (1 << 30);
    // For the support of coinbase, we will the bootloader formal address for now
    address public coinbase = BOOTLOADER_FORMAL_ADDRESS;
    // For consistency with other L2s
    uint256 public difficulty = 2500000000000000;
    uint256 public msize = (1 << 24);
    uint256 public baseFee;
    
    uint256 constant BLOCK_INFO_BLOCK_NUMBER_PART = (1<<128);
    // 2^128 * block_number + block_timestamp
    uint256 public currentBlockInfo;

    mapping(uint256 => bytes32) public blockHash;

    function setTxOrigin(address _newOrigin) external {
        origin = _newOrigin;
    }

    function setGasPrice(uint256 _gasPrice) external onlyBootloader {
        gasPrice = _gasPrice;
    }

    function getBlockHashEVM(uint256 _block) external view returns (bytes32 hash) {
        if(block.number < _block || block.number - _block > 256) {
            hash = bytes32(0);
        } else {
            hash = blockHash[_block];
        }
    }

    function getBlockNumberAndTimestamp() public view returns (uint256 blockNumber, uint256 blockTimestamp) {
        uint256 blockInfo = currentBlockInfo;
        blockNumber = blockInfo / BLOCK_INFO_BLOCK_NUMBER_PART;
        blockTimestamp = blockInfo % BLOCK_INFO_BLOCK_NUMBER_PART;
    }

    // Note, that for now, the implementation of the bootloader allows this variables to 
    // be incremented multiple times inside a block, so it should not relied upon right now.
    function getBlockNumber() public view returns (uint256 blockNumber) {
        (blockNumber, ) = getBlockNumberAndTimestamp();
    }

    function getBlockTimestamp() public view returns (uint256 timestamp) {
        (, timestamp) = getBlockNumberAndTimestamp();
    }
}
