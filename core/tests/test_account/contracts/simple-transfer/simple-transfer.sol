// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SimpleTransfer {
    address public owner;

    constructor() {
        owner = msg.sender; // Set the contract creator as the owner
    }

    // This function allows the contract to receive Ether with msg.data being empty
    receive() external payable {}

    modifier onlyOwner() {
        require(msg.sender == owner, "Only the owner can execute this.");
        _;
    }

    // Function to withdraw Ether to the owner's address
    function withdraw(uint _amount) public onlyOwner {
        require(address(this).balance >= _amount, "Insufficient balance in contract");
        payable(owner).transfer(_amount);
    }

    // Function to transfer Ether from this contract to any address
    function transfer(address _to, uint _amount) public onlyOwner {
        require(address(this).balance >= _amount, "Insufficient balance in contract");
        payable(_to).transfer(_amount);
    }

    // Function to check the contract's balance
    function getBalance() public view returns (uint) {
        return address(this).balance;
    }
}
