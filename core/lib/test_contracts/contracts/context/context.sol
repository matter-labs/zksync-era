// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract Context {
    function getBlockNumber() public view returns (uint256) {
        return block.number;
    }

    function getBlockTimestamp() public view returns (uint256) {
        return block.timestamp;
    }

    function getBlockGasLimit() public view returns (uint256) {
        return block.gaslimit;
    }

    function getTxGasPrice() public view returns (uint256) {
        return tx.gasprice;
    }

    function checkBlockNumber(uint256 fromBlockNumber, uint256 toBlockNumber) public {
        require(fromBlockNumber <= block.number && block.number <= toBlockNumber, "block number is out of range");
    }

    function checkBlockTimestamp(uint256 fromTimestamp, uint256 toTimestamp) public {
        require(fromTimestamp <= block.timestamp && block.timestamp <= toTimestamp, "block timestamp is out of range");
    }

    function checkTxOrigin(address expectedOrigin) public {
        require(tx.origin == expectedOrigin, "tx.origin is invalid");
    }

    function getBaseFee() public view returns (uint256) {
        return block.basefee;
    }

    function requireMsgValue(uint256 _requiredValue) external payable {
        require(msg.value == _requiredValue);
    }

    uint256 public valueOnCreate;

    constructor() payable {
        valueOnCreate = msg.value;
    }
}
