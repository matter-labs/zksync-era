import { BigNumber, BigNumberish, ethers, utils } from 'ethers';

interface CallDataParams {
    constructorCall?: boolean;
}

interface CallData extends CallDataParams {
    hash?: BigNumberish;
    input: BigNumberish[];
}

const CONSTRUCTOR_DATA_OFFSET = 8;
const FIELD_SIZE = 32;

const SELECTOR_SIZE_BYTES = 4;
const OFFSET_DATA = 1;
const OFFSET_HEADER = 0;

function toLeBytes(x: BigNumberish): Uint8Array {
    const hexString = BigNumber.from(x).toHexString();
    return utils.arrayify(hexString).reverse();
}

// This function parses calldata generated for a solidity contract call.
// Format is described in details here: https://docs.soliditylang.org/en/latest/abi-spec.html
// This function might incorrectly handle complex types.
export function parseCalldata(calldata: ethers.BytesLike, params?: CallDataParams): CallData {
    const bytes = utils.arrayify(calldata);

    // The first four bytes of the call data for a function call specifies the function to be called.
    // It is the first four bytes of the Keccak-256 hash of the signature of the function.
    if (bytes.length < 4) {
        throw new Error('No function selector found');
    }

    const selector = utils.hexlify(bytes.slice(0, 4));

    // All the arguments follow the selector and are encoded as defined in the ABI spec.
    // Arguments are aligned to 32 bytes each.
    if (bytes.length % 32 !== 4) {
        throw new Error('Unsupported arguments alignment');
    }

    const input = [];

    for (let i = 4; i < bytes.length; i += 32) {
        input.push(utils.hexlify(bytes.slice(i, i + 32)));
    }

    return {
        hash: selector,
        input,
        ...params
    };
}

// Spec: https://www.notion.so/matterlabs/Contract-ABI-21cfe71b2e3346029f4b591ae33332b4
export function calldataBytes(calldata: CallData): Uint8Array {
    let buffer: Uint8Array;
    let calldataSize: number;

    if (calldata.constructorCall) {
        const size = (OFFSET_DATA + calldata.input.length) * FIELD_SIZE;
        buffer = new Uint8Array(size);

        buffer[CONSTRUCTOR_DATA_OFFSET] |= 0b00000001;

        let calldataOffset = OFFSET_DATA * FIELD_SIZE;
        calldata.input.forEach((value) => {
            toLeBytes(value).forEach((byte, index) => {
                buffer[index + calldataOffset] = byte;
            });
            calldataOffset += FIELD_SIZE;
        });

        calldataSize = calldata.input.length * FIELD_SIZE;
    } else {
        const size = (OFFSET_DATA + 1 + calldata.input.length) * FIELD_SIZE;
        buffer = new Uint8Array(size);

        const entryHashOffset = (OFFSET_DATA + 1) * FIELD_SIZE - SELECTOR_SIZE_BYTES;
        toLeBytes(calldata.hash).forEach((byte, index) => {
            buffer[index + entryHashOffset] = byte;
        });

        for (let i = 0; i < calldata.input.length; i++) {
            const offset = (OFFSET_DATA + i) * FIELD_SIZE;
            const argument = toLeBytes(calldata.input[i]);

            buffer.set(argument.slice(SELECTOR_SIZE_BYTES), offset);
            buffer.set(argument.slice(0, SELECTOR_SIZE_BYTES), offset + 2 * FIELD_SIZE - SELECTOR_SIZE_BYTES);
        }

        calldataSize = SELECTOR_SIZE_BYTES + calldata.input.length * FIELD_SIZE;
    }

    const calldataSizeOffset = OFFSET_HEADER * FIELD_SIZE;
    toLeBytes(calldataSize).forEach((byte, index) => {
        buffer[calldataSizeOffset + index] = byte;
    });

    return buffer;
}
