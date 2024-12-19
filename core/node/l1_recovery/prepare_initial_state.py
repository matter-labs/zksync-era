import json
import psycopg2
import csv

chain_name = "V25"
# Connect to PostgreSQL
conn = psycopg2.connect(
    dbname="zksync_server_localhost_era",
    user="postgres",
    password="notsecurepassword",
    host="localhost",
)
cur = conn.cursor()

# Helper function to format binary data
def format_binary_data(value):
    if isinstance(value, memoryview):  # Check if the value is a binary data type
        hex_value = value.tobytes().hex()  # Convert to hex string
        return f"0x{hex_value.upper()}"  # Format as PostgreSQL bytea format
    return value

# Execute the query
storage_logs_query = """
SELECT hashed_key, address, key, value, operation_number, miniblocks.number
FROM storage_logs
LEFT JOIN miniblocks ON storage_logs.miniblock_number = miniblocks.number
LEFT JOIN l1_batches ON miniblocks.l1_batch_number = l1_batches.number
WHERE miniblocks.l1_batch_number = 0
ORDER BY operation_number;
"""

factory_deps_query = """
SELECT bytecode_hash, bytecode
FROM factory_deps
WHERE miniblock_number = 0
ORDER BY bytecode_hash;
"""


# Write results to JSON file
with open(f'initial_states/InitialState{chain_name}.json', 'w') as output_file:
    cur.execute(storage_logs_query)
    storage_logs_rows = cur.fetchall()

    # List to store each row as a dictionary
    storage_logs_list = []
    for row in storage_logs_rows:
        # Convert row to dictionary
        row_dict = {
            "hashed_key": format_binary_data(row[0]),
            "address": format_binary_data(row[1]),
            "key": format_binary_data(row[2]),
            "value": format_binary_data(row[3]),
            "operation_number": row[4],
        }
        storage_logs_list.append(row_dict)

    cur.execute(factory_deps_query)
    factory_deps_rows = cur.fetchall()
    factory_deps_list = []
    for row in factory_deps_rows:
        row_dict = {
            "bytecode_hash": format_binary_data(row[0]),
            "bytecode": format_binary_data(row[1])
        }
        factory_deps_list.append(row_dict)

    cur.execute(storage_logs_query)

    output_object = {
        "storage_logs": storage_logs_list,
        "factory_deps": factory_deps_list,
    }
    json.dump(output_object, output_file, indent=4)

# Close the connection
cur.close()
conn.close()

if __name__ == "__main__":
    pass
