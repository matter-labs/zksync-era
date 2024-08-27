// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract OpcodeTest {
    function execute() external {
        uint256 loaded = 1;
        uint256 tmp;
        uint256 prevBlock = block.number - 1;
        assembly {
            loaded := add(loaded, 1)
            loaded := mul(loaded, 2)
            loaded := sub(loaded, 1)
            loaded := div(loaded, 2)
            loaded := sdiv(loaded, 2)
            loaded := mod(loaded, 2)
            // ADDMOD
            // MULMOD
            loaded := exp(loaded, 2)
            loaded := signextend(loaded, 2)
            tmp := lt(loaded, 2)
            tmp := gt(loaded, 2)
            tmp := slt(loaded, 2)
            tmp := sgt(loaded, 2)
            tmp := eq(loaded, 2)
            tmp := iszero(tmp)
            tmp := and(1, 1)
            tmp := or(1, 1)
            tmp := xor(1, 1)
            tmp := not(tmp)
            tmp := byte(tmp, 1)
            tmp := shl(tmp, 1)
            tmp := shr(tmp, 1)
            tmp := sar(tmp, 1)
            tmp := keccak256(0, 0x40)
            tmp := address()
            tmp := balance(0x00)
            tmp := origin()
            tmp := caller()
            tmp := callvalue()
            // CALLDATALOAD
            tmp := calldatasize()
            // CALLDATACOPY
            tmp := codesize()
            // CODECOPY
            tmp := gasprice()
            // EXTCODESIZE
            // EXTCODECOPY
            tmp := returndatasize()
            // RETURNDATACOPY
            // EXTCODEHASH
            tmp := blockhash(prevBlock)
            tmp := coinbase()
            tmp := timestamp()
            tmp := number()
            tmp := prevrandao()
            tmp := gaslimit()
            tmp := chainid()
            tmp := selfbalance()
            tmp := basefee()
            // POP
            tmp := mload(1)
            mstore(1024, 1)
            mstore8(10242, 1)
            tmp := sload(0)
            sstore(0, 1)
            // JUMP
            // JUMPI
            // PC
            tmp := msize()
            tmp := gas()
            // JUMPDEST
            // PUSH0...PUSH32
            // DUP1...DUP16
            // SWAP1...SWAP16
            // LOG0...LOG4
            // CREATE
            // CALL
            // CALLCODE
            // RETURN
            // DELEGATECALL
            // CREATE2
            // STATICCALL
            // REVERT
            // INVALID
            // selfdestruct(sender)
        }

        // tmp = 0;
        // tmp = 0x11;
        // tmp = 0x2211;
        // tmp = 0x332211;
        // tmp = 0x44332211;
        // tmp = 0x5544332211;
        // tmp = 0x665544332211;
        // tmp = 0x77665544332211;
        // tmp = 0x8877665544332211;
        // tmp = 0x998877665544332211;
        // tmp = 0xaa998877665544332211;
        // tmp = 0xbbaa998877665544332211;
        // tmp = 0xccbbaa998877665544332211;
        // tmp = 0xddccbbaa998877665544332211;
        // tmp = 0xeeddccbbaa998877665544332211;
        // tmp = 0xffeeddccbbaa998877665544332211;
        // tmp = 0x11ffeeddccbbaa998877665544332211;
        // tmp = 0x2211ffeeddccbbaa998877665544332211;
        // tmp = 0x332211ffeeddccbbaa998877665544332211;
        // tmp = 0x44332211ffeeddccbbaa998877665544332211;
        // tmp = uint256(uint160(0x5544332211FFeeDDCcbbAa998877665544332211));
        // tmp = 0x665544332211ffeeddccbbaa998877665544332211;
        // tmp = 0x77665544332211ffeeddccbbaa998877665544332211;
        // tmp = 0x8877665544332211ffeeddccbbaa998877665544332211;
        // tmp = 0x998877665544332211ffeeddccbbaa998877665544332211;
        // tmp = 0xff998877665544332211ffeeddccbbaa998877665544332211;
        // tmp = 0x11ff998877665544332211ffeeddccbbaa998877665544332211;
        // tmp = 0x2211ff998877665544332211ffeeddccbbaa998877665544332211;
        // tmp = 0x332211ff998877665544332211ffeeddccbbaa998877665544332211;
        // tmp = 0x44332211ff998877665544332211ffeeddccbbaa998877665544332211;
        // tmp = 0x5544332211ff998877665544332211ffeeddccbbaa998877665544332211;
        // tmp = 0x665544332211ff998877665544332211ffeeddccbbaa998877665544332211;
        // tmp = 0x77665544332211ff998877665544332211ffeeddccbbaa998877665544332211;
    }
}
