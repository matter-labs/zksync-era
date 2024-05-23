file_path = "contracts/system-contracts/contracts-preprocessed/artifacts/EvmInterpreterPreprocessed.yul.zbin"
f = open(file_path, "r")
output_file_path = "bytecode"

with open(file_path, 'r') as file:
    with open(output_file_path, 'ab') as output_file:
        byte = file.read(2)
        while byte:
            if byte == "0x":
                byte = file.read(2)
                continue

            output_file.write(int(byte, 16).to_bytes(1, byteorder = "big", signed = False))
            byte = file.read(2)
