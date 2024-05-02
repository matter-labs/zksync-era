IR:

IR:

/// @use-src 0:"evm-contracts/ProxyCaller.sol"
object "ProxyCaller_61" {
    code {
        /// @src 0:254:988  "contract ProxyCaller {..."
        mstore(64, memoryguard(128))
        if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }

        constructor_ProxyCaller_61()

        let _1 := allocate_unbounded()
        codecopy(_1, dataoffset("ProxyCaller_61_deployed"), datasize("ProxyCaller_61_deployed"))

        return(_1, datasize("ProxyCaller_61_deployed"))

        function allocate_unbounded() -> memPtr {
            memPtr := mload(64)
        }

        function revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() {
            revert(0, 0)
        }

        /// @src 0:254:988  "contract ProxyCaller {..."
        function constructor_ProxyCaller_61() {

            /// @src 0:254:988  "contract ProxyCaller {..."

        }
        /// @src 0:254:988  "contract ProxyCaller {..."

    }
    /// @use-src 0:"evm-contracts/ProxyCaller.sol"
    object "ProxyCaller_61_deployed" {
        code {
            /// @src 0:254:988  "contract ProxyCaller {..."
            mstore(64, memoryguard(128))

            if iszero(lt(calldatasize(), 4))
            {
                let selector := shift_right_224_unsigned(calldataload(0))
                switch selector

                case 0x871fee31
                {
                    // proxyGetBytes(address)

                    external_fun_proxyGetBytes_60()
                }

                case 0xb16c2bb0
                {
                    // proxyGet(address)

                    external_fun_proxyGet_46()
                }

                case 0xc12d3c84
                {
                    // executeIncrement(address,uint256)

                    external_fun_executeIncrement_32()
                }

                default {}
            }

            revert_error_42b3090547df1d2001c96683413b8cf91c1b902ef5e3cb8d9f6f304cf7446f74()

            function shift_right_224_unsigned(value) -> newValue {
                newValue :=

                shr(224, value)

            }

            function allocate_unbounded() -> memPtr {
                memPtr := mload(64)
            }

            function revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() {
                revert(0, 0)
            }

            function revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() {
                revert(0, 0)
            }

            function revert_error_c1322bf8034eace5e0b5c7295db60986aa89aae5e0ea0873e4689e076861a5db() {
                revert(0, 0)
            }

            function cleanup_t_uint160(value) -> cleaned {
                cleaned := and(value, 0xffffffffffffffffffffffffffffffffffffffff)
            }

            function cleanup_t_address(value) -> cleaned {
                cleaned := cleanup_t_uint160(value)
            }

            function validator_revert_t_address(value) {
                if iszero(eq(value, cleanup_t_address(value))) { revert(0, 0) }
            }

            function abi_decode_t_address(offset, end) -> value {
                value := calldataload(offset)
                validator_revert_t_address(value)
            }

            function abi_decode_tuple_t_address(headStart, dataEnd) -> value0 {
                if slt(sub(dataEnd, headStart), 32) { revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() }

                {

                    let offset := 0

                    value0 := abi_decode_t_address(add(headStart, offset), dataEnd)
                }

            }

            function array_length_t_bytes_memory_ptr(value) -> length {

                length := mload(value)

            }

            function array_storeLengthForEncoding_t_bytes_memory_ptr_fromStack(pos, length) -> updated_pos {
                mstore(pos, length)
                updated_pos := add(pos, 0x20)
            }

            function copy_memory_to_memory_with_cleanup(src, dst, length) {
                let i := 0
                for { } lt(i, length) { i := add(i, 32) }
                {
                    mstore(add(dst, i), mload(add(src, i)))
                }
                mstore(add(dst, length), 0)
            }

            function round_up_to_mul_of_32(value) -> result {
                result := and(add(value, 31), not(31))
            }

            function abi_encode_t_bytes_memory_ptr_to_t_bytes_memory_ptr_fromStack(value, pos) -> end {
                let length := array_length_t_bytes_memory_ptr(value)
                pos := array_storeLengthForEncoding_t_bytes_memory_ptr_fromStack(pos, length)
                copy_memory_to_memory_with_cleanup(add(value, 0x20), pos, length)
                end := add(pos, round_up_to_mul_of_32(length))
            }

            function abi_encode_tuple_t_bytes_memory_ptr__to_t_bytes_memory_ptr__fromStack(headStart , value0) -> tail {
                tail := add(headStart, 32)

                mstore(add(headStart, 0), sub(tail, headStart))
                tail := abi_encode_t_bytes_memory_ptr_to_t_bytes_memory_ptr_fromStack(value0,  tail)

            }

            function external_fun_proxyGetBytes_60() {

                if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }
                let param_0 :=  abi_decode_tuple_t_address(4, calldatasize())
                let ret_0 :=  fun_proxyGetBytes_60(param_0)
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_tuple_t_bytes_memory_ptr__to_t_bytes_memory_ptr__fromStack(memPos , ret_0)
                return(memPos, sub(memEnd, memPos))

            }

            function cleanup_t_uint256(value) -> cleaned {
                cleaned := value
            }

            function abi_encode_t_uint256_to_t_uint256_fromStack(value, pos) {
                mstore(pos, cleanup_t_uint256(value))
            }

            function abi_encode_tuple_t_uint256__to_t_uint256__fromStack(headStart , value0) -> tail {
                tail := add(headStart, 32)

                abi_encode_t_uint256_to_t_uint256_fromStack(value0,  add(headStart, 0))

            }

            function external_fun_proxyGet_46() {

                if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }
                let param_0 :=  abi_decode_tuple_t_address(4, calldatasize())
                let ret_0 :=  fun_proxyGet_46(param_0)
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_tuple_t_uint256__to_t_uint256__fromStack(memPos , ret_0)
                return(memPos, sub(memEnd, memPos))

            }

            function validator_revert_t_uint256(value) {
                if iszero(eq(value, cleanup_t_uint256(value))) { revert(0, 0) }
            }

            function abi_decode_t_uint256(offset, end) -> value {
                value := calldataload(offset)
                validator_revert_t_uint256(value)
            }

            function abi_decode_tuple_t_addresst_uint256(headStart, dataEnd) -> value0, value1 {
                if slt(sub(dataEnd, headStart), 64) { revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() }

                {

                    let offset := 0

                    value0 := abi_decode_t_address(add(headStart, offset), dataEnd)
                }

                {

                    let offset := 32

                    value1 := abi_decode_t_uint256(add(headStart, offset), dataEnd)
                }

            }

            function abi_encode_tuple__to__fromStack(headStart ) -> tail {
                tail := add(headStart, 0)

            }

            function external_fun_executeIncrement_32() {

                if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }
                let param_0, param_1 :=  abi_decode_tuple_t_addresst_uint256(4, calldatasize())
                fun_executeIncrement_32(param_0, param_1)
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_tuple__to__fromStack(memPos  )
                return(memPos, sub(memEnd, memPos))

            }

            function revert_error_42b3090547df1d2001c96683413b8cf91c1b902ef5e3cb8d9f6f304cf7446f74() {
                revert(0, 0)
            }

            function zero_value_for_split_t_bytes_memory_ptr() -> ret {
                ret := 96
            }

            function identity(value) -> ret {
                ret := value
            }

            function convert_t_uint160_to_t_uint160(value) -> converted {
                converted := cleanup_t_uint160(identity(cleanup_t_uint160(value)))
            }

            function convert_t_uint160_to_t_contract$_ICounterWithParam_$17(value) -> converted {
                converted := convert_t_uint160_to_t_uint160(value)
            }

            function convert_t_address_to_t_contract$_ICounterWithParam_$17(value) -> converted {
                converted := convert_t_uint160_to_t_contract$_ICounterWithParam_$17(value)
            }

            function convert_t_uint160_to_t_address(value) -> converted {
                converted := convert_t_uint160_to_t_uint160(value)
            }

            function convert_t_contract$_ICounterWithParam_$17_to_t_address(value) -> converted {
                converted := convert_t_uint160_to_t_address(value)
            }

            function revert_error_0cc013b6b3b6beabea4e3a74a6d380f0df81852ca99887912475e1f66b2a2c20() {
                revert(0, 0)
            }

            function panic_error_0x41() {
                mstore(0, 35408467139433450592217433187231851964531694900788300625387963629091585785856)
                mstore(4, 0x41)
                revert(0, 0x24)
            }

            function finalize_allocation(memPtr, size) {
                let newFreePtr := add(memPtr, round_up_to_mul_of_32(size))
                // protect against overflow
                if or(gt(newFreePtr, 0xffffffffffffffff), lt(newFreePtr, memPtr)) { panic_error_0x41() }
                mstore(64, newFreePtr)
            }

            function shift_left_224(value) -> newValue {
                newValue :=

                shl(224, value)

            }

            function revert_error_1b9f4a0a5773e33b91aa01db23bf8c55fce1411167c872835e7fa00a4f17d46d() {
                revert(0, 0)
            }

            function revert_error_987264b3b1d58a9c7f8255e93e81c77d86d6299019c33110a076957a3e06e2ae() {
                revert(0, 0)
            }

            function allocate_memory(size) -> memPtr {
                memPtr := allocate_unbounded()
                finalize_allocation(memPtr, size)
            }

            function array_allocation_size_t_bytes_memory_ptr(length) -> size {
                // Make sure we can allocate memory without overflow
                if gt(length, 0xffffffffffffffff) { panic_error_0x41() }

                size := round_up_to_mul_of_32(length)

                // add length slot
                size := add(size, 0x20)

            }

            function abi_decode_available_length_t_bytes_memory_ptr_fromMemory(src, length, end) -> array {
                array := allocate_memory(array_allocation_size_t_bytes_memory_ptr(length))
                mstore(array, length)
                let dst := add(array, 0x20)
                if gt(add(src, length), end) { revert_error_987264b3b1d58a9c7f8255e93e81c77d86d6299019c33110a076957a3e06e2ae() }
                copy_memory_to_memory_with_cleanup(src, dst, length)
            }

            // bytes
            function abi_decode_t_bytes_memory_ptr_fromMemory(offset, end) -> array {
                if iszero(slt(add(offset, 0x1f), end)) { revert_error_1b9f4a0a5773e33b91aa01db23bf8c55fce1411167c872835e7fa00a4f17d46d() }
                let length := mload(offset)
                array := abi_decode_available_length_t_bytes_memory_ptr_fromMemory(add(offset, 0x20), length, end)
            }

            function abi_decode_tuple_t_bytes_memory_ptr_fromMemory(headStart, dataEnd) -> value0 {
                if slt(sub(dataEnd, headStart), 32) { revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() }

                {

                    let offset := mload(add(headStart, 0))
                    if gt(offset, 0xffffffffffffffff) { revert_error_c1322bf8034eace5e0b5c7295db60986aa89aae5e0ea0873e4689e076861a5db() }

                    value0 := abi_decode_t_bytes_memory_ptr_fromMemory(add(headStart, offset), dataEnd)
                }

            }

            function revert_forward_1() {
                let pos := allocate_unbounded()
                returndatacopy(pos, 0, returndatasize())
                revert(pos, returndatasize())
            }

            /// @ast-id 60
            /// @src 0:834:986  "function proxyGetBytes(..."
            function fun_proxyGetBytes_60(var_dest_48) -> var_returnData_51_mpos {
                /// @src 0:903:926  "bytes memory returnData"
                let zero_t_bytes_memory_ptr_1_mpos := zero_value_for_split_t_bytes_memory_ptr()
                var_returnData_51_mpos := zero_t_bytes_memory_ptr_1_mpos

                /// @src 0:963:967  "dest"
                let _2 := var_dest_48
                let expr_54 := _2
                /// @src 0:945:968  "ICounterWithParam(dest)"
                let expr_55_address := convert_t_address_to_t_contract$_ICounterWithParam_$17(expr_54)
                /// @src 0:945:977  "ICounterWithParam(dest).getBytes"
                let expr_56_address := convert_t_contract$_ICounterWithParam_$17_to_t_address(expr_55_address)
                let expr_56_functionSelector := 0x0bcd3b33
                /// @src 0:945:979  "ICounterWithParam(dest).getBytes()"

                // storage for arguments and returned data
                let _3 := allocate_unbounded()
                mstore(_3, shift_left_224(expr_56_functionSelector))
                let _4 := abi_encode_tuple__to__fromStack(add(_3, 4) )

                let _5 := call(gas(), expr_56_address,  0,  _3, sub(_4, _3), _3, 0)

                if iszero(_5) { revert_forward_1() }

                let expr_57_mpos
                if _5 {

                    let _6 := returndatasize()
                    returndatacopy(_3, 0, _6)

                    // update freeMemoryPointer according to dynamic return size
                    finalize_allocation(_3, _6)

                    // decode return parameters from external try-call into retVars
                    expr_57_mpos :=  abi_decode_tuple_t_bytes_memory_ptr_fromMemory(_3, add(_3, _6))
                }
                /// @src 0:938:979  "return ICounterWithParam(dest).getBytes()"
                var_returnData_51_mpos := expr_57_mpos
                leave

            }
            /// @src 0:254:988  "contract ProxyCaller {..."

            function zero_value_for_split_t_uint256() -> ret {
                ret := 0
            }

            function abi_decode_t_uint256_fromMemory(offset, end) -> value {
                value := mload(offset)
                validator_revert_t_uint256(value)
            }

            function abi_decode_tuple_t_uint256_fromMemory(headStart, dataEnd) -> value0 {
                if slt(sub(dataEnd, headStart), 32) { revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() }

                {

                    let offset := 0

                    value0 := abi_decode_t_uint256_fromMemory(add(headStart, offset), dataEnd)
                }

            }

            /// @ast-id 46
            /// @src 0:400:828  "function proxyGet(address dest) external view returns (uint256) {..."
            function fun_proxyGet_46(var_dest_34) -> var__37 {
                /// @src 0:455:462  "uint256"
                let zero_t_uint256_7 := zero_value_for_split_t_uint256()
                var__37 := zero_t_uint256_7

                /// @src 0:810:814  "dest"
                let _8 := var_dest_34
                let expr_40 := _8
                /// @src 0:792:815  "ICounterWithParam(dest)"
                let expr_41_address := convert_t_address_to_t_contract$_ICounterWithParam_$17(expr_40)
                /// @src 0:792:819  "ICounterWithParam(dest).get"
                let expr_42_address := convert_t_contract$_ICounterWithParam_$17_to_t_address(expr_41_address)
                let expr_42_functionSelector := 0x6d4ce63c
                /// @src 0:792:821  "ICounterWithParam(dest).get()"

                // storage for arguments and returned data
                let _9 := allocate_unbounded()
                mstore(_9, shift_left_224(expr_42_functionSelector))
                let _10 := abi_encode_tuple__to__fromStack(add(_9, 4) )

                let _11 := staticcall(gas(), expr_42_address,  _9, sub(_10, _9), _9, 32)

                if iszero(_11) { revert_forward_1() }

                let expr_43
                if _11 {

                    let _12 := 32

                    if gt(_12, returndatasize()) {
                        _12 := returndatasize()
                    }

                    // update freeMemoryPointer according to dynamic return size
                    finalize_allocation(_9, _12)

                    // decode return parameters from external try-call into retVars
                    expr_43 :=  abi_decode_tuple_t_uint256_fromMemory(_9, add(_9, _12))
                }
                /// @src 0:785:821  "return ICounterWithParam(dest).get()"
                var__37 := expr_43
                leave

            }
            /// @src 0:254:988  "contract ProxyCaller {..."

            function abi_decode_tuple__fromMemory(headStart, dataEnd)   {
                if slt(sub(dataEnd, headStart), 0) { revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() }

            }

            /// @ast-id 32
            /// @src 0:281:394  "function executeIncrement(address dest, uint256 x) external {..."
            function fun_executeIncrement_32(var_dest_19, var_x_21) {

                /// @src 0:369:373  "dest"
                let _13 := var_dest_19
                let expr_25 := _13
                /// @src 0:351:374  "ICounterWithParam(dest)"
                let expr_26_address := convert_t_address_to_t_contract$_ICounterWithParam_$17(expr_25)
                /// @src 0:351:384  "ICounterWithParam(dest).increment"
                let expr_27_address := convert_t_contract$_ICounterWithParam_$17_to_t_address(expr_26_address)
                let expr_27_functionSelector := 0x7cf5dab0
                /// @src 0:385:386  "x"
                let _14 := var_x_21
                let expr_28 := _14
                /// @src 0:351:387  "ICounterWithParam(dest).increment(x)"

                if iszero(extcodesize(expr_27_address)) { revert_error_0cc013b6b3b6beabea4e3a74a6d380f0df81852ca99887912475e1f66b2a2c20() }

                // storage for arguments and returned data
                let _15 := allocate_unbounded()
                mstore(_15, shift_left_224(expr_27_functionSelector))
                let _16 := abi_encode_tuple_t_uint256__to_t_uint256__fromStack(add(_15, 4) , expr_28)

                let _17 := call(gas(), expr_27_address,  0,  _15, sub(_16, _15), _15, 0)

                if iszero(_17) { revert_forward_1() }

                if _17 {

                    let _18 := 0

                    if gt(_18, returndatasize()) {
                        _18 := returndatasize()
                    }

                    // update freeMemoryPointer according to dynamic return size
                    finalize_allocation(_15, _18)

                    // decode return parameters from external try-call into retVars
                    abi_decode_tuple__fromMemory(_15, add(_15, _18))
                }

            }
            /// @src 0:254:988  "contract ProxyCaller {..."

        }

        data ".metadata" hex"a264697066735822122060685ebede084efb214ceed5616f85551e60ac2a3b1587deb3a69895aaf56e6b64736f6c63430008180033"
    }

}


