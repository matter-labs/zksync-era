// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract TransferTest {
    function transfer(address payable to, uint256 amount) public payable {
        to.transfer(amount);
    }

    function send(address payable to, uint256 amount) public payable {
        bool success = to.send(amount);

        require(success, "Transaction failed");
    }
    
    receive() external payable {

    }
}

contract Recipient {
    event Received(address indexed sender, uint256 amount);

    receive() external payable {
        require(gasleft() >= 2100, "Not enough gas");
        require(gasleft() <= 2300, "Too much gas");
        emit Received(msg.sender, msg.value);
    }
}

contract ReentrantRecipient {
    uint256 x;

    event Received(uint256 tmp, uint256 amount);

    function setX() external payable {
        x = 1;
    }

    receive() external payable {
        require(gasleft() >= 15000);
        x = 1;
    }
}
