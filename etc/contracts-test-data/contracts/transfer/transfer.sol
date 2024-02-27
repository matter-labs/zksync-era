// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract TransferTest {
    function transfer(address payable to, uint256 amount) public {
        to.transfer(amount);
    }

    function send(address payable to, uint256 amount) public {
        bool success = to.send(amount);
        require(success, "Transfer failed");
    }
}

contract Recipient {
    event Received(address indexed sender, uint256 amount);

    receive() external payable {
        emit Received(msg.sender, msg.value);
    }
}
