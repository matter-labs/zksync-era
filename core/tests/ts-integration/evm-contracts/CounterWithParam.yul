IR:

/// @use-src 0:"evm-contracts/CounterWithParam.sol"
object "CounterWithParam_88" {
    code {
        /// @src 0:66:893  "contract CounterWithParam {..."
        mstore(64, memoryguard(128))
        if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }

        let _1 := copy_arguments_for_constructor_13_object_CounterWithParam_88()
        constructor_CounterWithParam_88(_1)

        let _2 := allocate_unbounded()
        codecopy(_2, dataoffset("CounterWithParam_88_deployed"), datasize("CounterWithParam_88_deployed"))

        return(_2, datasize("CounterWithParam_88_deployed"))

        function allocate_unbounded() -> memPtr {
            memPtr := mload(64)
        }

        function revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() {
            revert(0, 0)
        }

        function round_up_to_mul_of_32(value) -> result {
            result := and(add(value, 31), not(31))
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

        function allocate_memory(size) -> memPtr {
            memPtr := allocate_unbounded()
            finalize_allocation(memPtr, size)
        }

        function revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() {
            revert(0, 0)
        }

        function revert_error_c1322bf8034eace5e0b5c7295db60986aa89aae5e0ea0873e4689e076861a5db() {
            revert(0, 0)
        }

        function cleanup_t_uint256(value) -> cleaned {
            cleaned := value
        }

        function validator_revert_t_uint256(value) {
            if iszero(eq(value, cleanup_t_uint256(value))) { revert(0, 0) }
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

        function copy_arguments_for_constructor_13_object_CounterWithParam_88() -> ret_param_0 {
            let programSize := datasize("CounterWithParam_88")
            let argSize := sub(codesize(), programSize)

            let memoryDataOffset := allocate_memory(argSize)
            codecopy(memoryDataOffset, programSize, argSize)

            ret_param_0 := abi_decode_tuple_t_uint256_fromMemory(memoryDataOffset, add(memoryDataOffset, argSize))
        }

        function shift_left_0(value) -> newValue {
            newValue :=

            shl(0, value)

        }

        function update_byte_slice_32_shift_0(value, toInsert) -> result {
            let mask := 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
            toInsert := shift_left_0(toInsert)
            value := and(value, not(mask))
            result := or(value, and(toInsert, mask))
        }

        function identity(value) -> ret {
            ret := value
        }

        function convert_t_uint256_to_t_uint256(value) -> converted {
            converted := cleanup_t_uint256(identity(cleanup_t_uint256(value)))
        }

        function prepare_store_t_uint256(value) -> ret {
            ret := value
        }

        function update_storage_value_offset_0t_uint256_to_t_uint256(slot, value_0) {
            let convertedValue_0 := convert_t_uint256_to_t_uint256(value_0)
            sstore(slot, update_byte_slice_32_shift_0(sload(slot), prepare_store_t_uint256(convertedValue_0)))
        }

        /// @ast-id 13
        /// @src 0:119:194  "constructor(uint256 _startingValue) {..."
        function constructor_CounterWithParam_88(var__startingValue_5) {

            /// @src 0:119:194  "constructor(uint256 _startingValue) {..."

            /// @src 0:173:187  "_startingValue"
            let _3 := var__startingValue_5
            let expr_9 := _3
            /// @src 0:165:187  "value = _startingValue"
            update_storage_value_offset_0t_uint256_to_t_uint256(0x00, expr_9)
            let expr_10 := expr_9

        }
        /// @src 0:66:893  "contract CounterWithParam {..."

    }
    /// @use-src 0:"evm-contracts/CounterWithParam.sol"
    object "CounterWithParam_88_deployed" {
        code {
            /// @src 0:66:893  "contract CounterWithParam {..."
            mstore(64, memoryguard(128))

            if iszero(lt(calldatasize(), 4))
            {
                let selector := shift_right_224_unsigned(calldataload(0))
                switch selector

                case 0x0436dad6
                {
                    // incrementWithRevert(uint256,bool)

                    external_fun_incrementWithRevert_61()
                }

                case 0x0bcd3b33
                {
                    // getBytes()

                    external_fun_getBytes_87()
                }

                case 0x60fe47b1
                {
                    // set(uint256)

                    external_fun_set_71()
                }

                case 0x6d4ce63c
                {
                    // get()

                    external_fun_get_79()
                }

                case 0x7cf5dab0
                {
                    // increment(uint256)

                    external_fun_increment_23()
                }

                case 0x8f9c4749
                {
                    // incrementWithRevertPayable(uint256,bool)

                    external_fun_incrementWithRevertPayable_38()
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

            function cleanup_t_uint256(value) -> cleaned {
                cleaned := value
            }

            function validator_revert_t_uint256(value) {
                if iszero(eq(value, cleanup_t_uint256(value))) { revert(0, 0) }
            }

            function abi_decode_t_uint256(offset, end) -> value {
                value := calldataload(offset)
                validator_revert_t_uint256(value)
            }

            function cleanup_t_bool(value) -> cleaned {
                cleaned := iszero(iszero(value))
            }

            function validator_revert_t_bool(value) {
                if iszero(eq(value, cleanup_t_bool(value))) { revert(0, 0) }
            }

            function abi_decode_t_bool(offset, end) -> value {
                value := calldataload(offset)
                validator_revert_t_bool(value)
            }

            function abi_decode_tuple_t_uint256t_bool(headStart, dataEnd) -> value0, value1 {
                if slt(sub(dataEnd, headStart), 64) { revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() }

                {

                    let offset := 0

                    value0 := abi_decode_t_uint256(add(headStart, offset), dataEnd)
                }

                {

                    let offset := 32

                    value1 := abi_decode_t_bool(add(headStart, offset), dataEnd)
                }

            }

            function abi_encode_t_uint256_to_t_uint256_fromStack(value, pos) {
                mstore(pos, cleanup_t_uint256(value))
            }

            function abi_encode_tuple_t_uint256__to_t_uint256__fromStack(headStart , value0) -> tail {
                tail := add(headStart, 32)

                abi_encode_t_uint256_to_t_uint256_fromStack(value0,  add(headStart, 0))

            }

            function external_fun_incrementWithRevert_61() {

                if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }
                let param_0, param_1 :=  abi_decode_tuple_t_uint256t_bool(4, calldatasize())
                let ret_0 :=  fun_incrementWithRevert_61(param_0, param_1)
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_tuple_t_uint256__to_t_uint256__fromStack(memPos , ret_0)
                return(memPos, sub(memEnd, memPos))

            }

            function abi_decode_tuple_(headStart, dataEnd)   {
                if slt(sub(dataEnd, headStart), 0) { revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() }

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

            function external_fun_getBytes_87() {

                if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }
                abi_decode_tuple_(4, calldatasize())
                let ret_0 :=  fun_getBytes_87()
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_tuple_t_bytes_memory_ptr__to_t_bytes_memory_ptr__fromStack(memPos , ret_0)
                return(memPos, sub(memEnd, memPos))

            }

            function abi_decode_tuple_t_uint256(headStart, dataEnd) -> value0 {
                if slt(sub(dataEnd, headStart), 32) { revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() }

                {

                    let offset := 0

                    value0 := abi_decode_t_uint256(add(headStart, offset), dataEnd)
                }

            }

            function abi_encode_tuple__to__fromStack(headStart ) -> tail {
                tail := add(headStart, 0)

            }

            function external_fun_set_71() {

                if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }
                let param_0 :=  abi_decode_tuple_t_uint256(4, calldatasize())
                fun_set_71(param_0)
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_tuple__to__fromStack(memPos  )
                return(memPos, sub(memEnd, memPos))

            }

            function external_fun_get_79() {

                if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }
                abi_decode_tuple_(4, calldatasize())
                let ret_0 :=  fun_get_79()
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_tuple_t_uint256__to_t_uint256__fromStack(memPos , ret_0)
                return(memPos, sub(memEnd, memPos))

            }

            function external_fun_increment_23() {

                if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }
                let param_0 :=  abi_decode_tuple_t_uint256(4, calldatasize())
                fun_increment_23(param_0)
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_tuple__to__fromStack(memPos  )
                return(memPos, sub(memEnd, memPos))

            }

            function external_fun_incrementWithRevertPayable_38() {

                let param_0, param_1 :=  abi_decode_tuple_t_uint256t_bool(4, calldatasize())
                let ret_0 :=  fun_incrementWithRevertPayable_38(param_0, param_1)
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_tuple_t_uint256__to_t_uint256__fromStack(memPos , ret_0)
                return(memPos, sub(memEnd, memPos))

            }

            function revert_error_42b3090547df1d2001c96683413b8cf91c1b902ef5e3cb8d9f6f304cf7446f74() {
                revert(0, 0)
            }

            function zero_value_for_split_t_uint256() -> ret {
                ret := 0
            }

            function shift_right_0_unsigned(value) -> newValue {
                newValue :=

                shr(0, value)

            }

            function cleanup_from_storage_t_uint256(value) -> cleaned {
                cleaned := value
            }

            function extract_from_storage_value_offset_0t_uint256(slot_value) -> value {
                value := cleanup_from_storage_t_uint256(shift_right_0_unsigned(slot_value))
            }

            function read_from_storage_split_offset_0_t_uint256(slot) -> value {
                value := extract_from_storage_value_offset_0t_uint256(sload(slot))

            }

            function panic_error_0x11() {
                mstore(0, 35408467139433450592217433187231851964531694900788300625387963629091585785856)
                mstore(4, 0x11)
                revert(0, 0x24)
            }

            function checked_add_t_uint256(x, y) -> sum {
                x := cleanup_t_uint256(x)
                y := cleanup_t_uint256(y)
                sum := add(x, y)

                if gt(x, sum) { panic_error_0x11() }

            }

            function shift_left_0(value) -> newValue {
                newValue :=

                shl(0, value)

            }

            function update_byte_slice_32_shift_0(value, toInsert) -> result {
                let mask := 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                toInsert := shift_left_0(toInsert)
                value := and(value, not(mask))
                result := or(value, and(toInsert, mask))
            }

            function identity(value) -> ret {
                ret := value
            }

            function convert_t_uint256_to_t_uint256(value) -> converted {
                converted := cleanup_t_uint256(identity(cleanup_t_uint256(value)))
            }

            function prepare_store_t_uint256(value) -> ret {
                ret := value
            }

            function update_storage_value_offset_0t_uint256_to_t_uint256(slot, value_0) {
                let convertedValue_0 := convert_t_uint256_to_t_uint256(value_0)
                sstore(slot, update_byte_slice_32_shift_0(sload(slot), prepare_store_t_uint256(convertedValue_0)))
            }

            function array_storeLengthForEncoding_t_string_memory_ptr_fromStack(pos, length) -> updated_pos {
                mstore(pos, length)
                updated_pos := add(pos, 0x20)
            }

            function store_literal_in_memory_37806cbbbe6373334747dff6c63a24076f006dbf934c864a3c6505dd85ba7686(memPtr) {

                mstore(add(memPtr, 0), "This method always reverts")

            }

            function abi_encode_t_stringliteral_37806cbbbe6373334747dff6c63a24076f006dbf934c864a3c6505dd85ba7686_to_t_string_memory_ptr_fromStack(pos) -> end {
                pos := array_storeLengthForEncoding_t_string_memory_ptr_fromStack(pos, 26)
                store_literal_in_memory_37806cbbbe6373334747dff6c63a24076f006dbf934c864a3c6505dd85ba7686(pos)
                end := add(pos, 32)
            }

            function abi_encode_tuple_t_stringliteral_37806cbbbe6373334747dff6c63a24076f006dbf934c864a3c6505dd85ba7686__to_t_string_memory_ptr__fromStack(headStart ) -> tail {
                tail := add(headStart, 32)

                mstore(add(headStart, 0), sub(tail, headStart))
                tail := abi_encode_t_stringliteral_37806cbbbe6373334747dff6c63a24076f006dbf934c864a3c6505dd85ba7686_to_t_string_memory_ptr_fromStack( tail)

            }

            /// @ast-id 61
            /// @src 0:439:659  "function incrementWithRevert(uint256 x, bool shouldRevert) public returns (uint256) {..."
            function fun_incrementWithRevert_61(var_x_40, var_shouldRevert_42) -> var__45 {
                /// @src 0:514:521  "uint256"
                let zero_t_uint256_1 := zero_value_for_split_t_uint256()
                var__45 := zero_t_uint256_1

                /// @src 0:542:543  "x"
                let _2 := var_x_40
                let expr_48 := _2
                /// @src 0:533:543  "value += x"
                let _3 := read_from_storage_split_offset_0_t_uint256(0x00)
                let expr_49 := checked_add_t_uint256(_3, expr_48)

                update_storage_value_offset_0t_uint256_to_t_uint256(0x00, expr_49)
                /// @src 0:556:568  "shouldRevert"
                let _4 := var_shouldRevert_42
                let expr_51 := _4
                /// @src 0:553:631  "if(shouldRevert) {..."
                if expr_51 {
                    /// @src 0:584:620  "revert(\"This method always reverts\")"
                    {
                        let _5 := allocate_unbounded()
                        mstore(_5, 3963877391197344453575983046348115674221700746820753546331534351508065746944)
                        let _6 := abi_encode_tuple_t_stringliteral_37806cbbbe6373334747dff6c63a24076f006dbf934c864a3c6505dd85ba7686__to_t_string_memory_ptr__fromStack(add(_5, 4) )
                        revert(_5, sub(_6, _5))
                    }/// @src 0:553:631  "if(shouldRevert) {..."
                }
                /// @src 0:647:652  "value"
                let _7 := read_from_storage_split_offset_0_t_uint256(0x00)
                let expr_58 := _7
                /// @src 0:640:652  "return value"
                var__45 := expr_58
                leave

            }
            /// @src 0:66:893  "contract CounterWithParam {..."

            function zero_value_for_split_t_bytes_memory_ptr() -> ret {
                ret := 96
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

            function allocate_memory(size) -> memPtr {
                memPtr := allocate_unbounded()
                finalize_allocation(memPtr, size)
            }

            function array_allocation_size_t_string_memory_ptr(length) -> size {
                // Make sure we can allocate memory without overflow
                if gt(length, 0xffffffffffffffff) { panic_error_0x41() }

                size := round_up_to_mul_of_32(length)

                // add length slot
                size := add(size, 0x20)

            }

            function allocate_memory_array_t_string_memory_ptr(length) -> memPtr {
                let allocSize := array_allocation_size_t_string_memory_ptr(length)
                memPtr := allocate_memory(allocSize)

                mstore(memPtr, length)

            }

            function store_literal_in_memory_5855073c29c25eb7c52ddac2136cfe1527eaf496347a6a693d8fe78129c187f3(memPtr) {

                mstore(add(memPtr, 0), "Testing")

            }

            function copy_literal_to_memory_5855073c29c25eb7c52ddac2136cfe1527eaf496347a6a693d8fe78129c187f3() -> memPtr {
                memPtr := allocate_memory_array_t_string_memory_ptr(7)
                store_literal_in_memory_5855073c29c25eb7c52ddac2136cfe1527eaf496347a6a693d8fe78129c187f3(add(memPtr, 32))
            }

            function convert_t_stringliteral_5855073c29c25eb7c52ddac2136cfe1527eaf496347a6a693d8fe78129c187f3_to_t_bytes_memory_ptr() -> converted {
                converted := copy_literal_to_memory_5855073c29c25eb7c52ddac2136cfe1527eaf496347a6a693d8fe78129c187f3()
            }

            /// @ast-id 87
            /// @src 0:808:891  "function getBytes() public returns (bytes memory) {..."
            function fun_getBytes_87() -> var__82_mpos {
                /// @src 0:844:856  "bytes memory"
                let zero_t_bytes_memory_ptr_8_mpos := zero_value_for_split_t_bytes_memory_ptr()
                var__82_mpos := zero_t_bytes_memory_ptr_8_mpos

                /// @src 0:868:884  "return \"Testing\""
                var__82_mpos := convert_t_stringliteral_5855073c29c25eb7c52ddac2136cfe1527eaf496347a6a693d8fe78129c187f3_to_t_bytes_memory_ptr()
                leave

            }
            /// @src 0:66:893  "contract CounterWithParam {..."

            /// @ast-id 71
            /// @src 0:665:722  "function set(uint256 x) public {..."
            function fun_set_71(var_x_63) {

                /// @src 0:714:715  "x"
                let _9 := var_x_63
                let expr_67 := _9
                /// @src 0:706:715  "value = x"
                update_storage_value_offset_0t_uint256_to_t_uint256(0x00, expr_67)
                let expr_68 := expr_67

            }
            /// @src 0:66:893  "contract CounterWithParam {..."

            /// @ast-id 79
            /// @src 0:728:802  "function get() public view returns (uint256) {..."
            function fun_get_79() -> var__74 {
                /// @src 0:764:771  "uint256"
                let zero_t_uint256_10 := zero_value_for_split_t_uint256()
                var__74 := zero_t_uint256_10

                /// @src 0:790:795  "value"
                let _11 := read_from_storage_split_offset_0_t_uint256(0x00)
                let expr_76 := _11
                /// @src 0:783:795  "return value"
                var__74 := expr_76
                leave

            }
            /// @src 0:66:893  "contract CounterWithParam {..."

            /// @ast-id 23
            /// @src 0:200:264  "function increment(uint256 x) public {..."
            function fun_increment_23(var_x_15) {

                /// @src 0:256:257  "x"
                let _12 := var_x_15
                let expr_19 := _12
                /// @src 0:247:257  "value += x"
                let _13 := read_from_storage_split_offset_0_t_uint256(0x00)
                let expr_20 := checked_add_t_uint256(_13, expr_19)

                update_storage_value_offset_0t_uint256_to_t_uint256(0x00, expr_20)

            }
            /// @src 0:66:893  "contract CounterWithParam {..."

            /// @ast-id 38
            /// @src 0:274:433  "function incrementWithRevertPayable(uint256 x, bool shouldRevert) payable public returns (uint256) {..."
            function fun_incrementWithRevertPayable_38(var_x_25, var_shouldRevert_27) -> var__30 {
                /// @src 0:364:371  "uint256"
                let zero_t_uint256_14 := zero_value_for_split_t_uint256()
                var__30 := zero_t_uint256_14

                /// @src 0:410:411  "x"
                let _15 := var_x_25
                let expr_33 := _15
                /// @src 0:413:425  "shouldRevert"
                let _16 := var_shouldRevert_27
                let expr_34 := _16
                /// @src 0:390:426  "incrementWithRevert(x, shouldRevert)"
                let expr_35 := fun_incrementWithRevert_61(expr_33, expr_34)
                /// @src 0:383:426  "return incrementWithRevert(x, shouldRevert)"
                var__30 := expr_35
                leave

            }
            /// @src 0:66:893  "contract CounterWithParam {..."

        }

        data ".metadata" hex"a2646970667358221220dbcf2c5da6e0a1418b176efe76c8a11403a6544509bbe0eaa382c8356da6536464736f6c63430008180033"
    }

}


