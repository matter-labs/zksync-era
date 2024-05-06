//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.8;

contract BytesWriter {
    bytes private message;

    constructor(bytes memory _message) {
        message = _message;
    }

    function readBytes() public view returns (bytes memory) {
        return message;
    }

    function writeBytes(bytes memory _message) public {
        message = _message;
    }

    // event StringBytesLengthMessage(address sender, string inputString, uint256 bytesLength);

    // function getStringBytesLength(string memory str) external {
    //     bytes memory strBytes = bytes(str);
    //     uint256 length = strBytes.length;

    //     emit StringBytesLengthMessage(msg.sender, str, length);
    // }
}
