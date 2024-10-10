// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract TransferTest {
    function transfer(address payable to, uint256 amount) public payable {
        (bool success, ) = to.call{value: amount}(""); // FIXME: revert; required because it's impossible to suppress errors
        require(success, "transfer reverted");
    }

    function send(address payable to, uint256 amount) public payable {
        (bool success, ) = to.call{value: amount}(""); // FIXME: revert; required because it's impossible to suppress errors
        require(success, "Transaction failed");
    }
    
    receive() external payable {
        // Do nothing
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
