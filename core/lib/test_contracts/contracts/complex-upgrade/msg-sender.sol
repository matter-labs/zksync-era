// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

contract MsgSenderTest {
    function testMsgSender(
        address _expectedSender
    ) external view {
        require(msg.sender == _expectedSender, "Wrong sender");
    }
}
