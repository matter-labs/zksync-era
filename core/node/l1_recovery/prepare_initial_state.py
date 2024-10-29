import psycopg2
import csv

chain_name = "Local"
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
        return f"E'\\\\x{hex_value.upper()}'"  # Format as PostgreSQL bytea format
    return value

# Execute the query
query = """
SELECT hashed_key, address, key, value, operation_number, miniblocks.number
FROM storage_logs
LEFT JOIN miniblocks ON storage_logs.miniblock_number = miniblocks.number
LEFT JOIN l1_batches ON miniblocks.l1_batch_number = l1_batches.number
WHERE miniblocks.l1_batch_number = 0
ORDER BY operation_number;
"""
cur.execute(query)

rows = cur.fetchall()

# Write results to CSV file
with open(f'InitialState{chain_name}.csv', 'w', newline='') as csvfile:
    csv_writer = csv.writer(csvfile)

    print(f"Writing {len(rows)} storage_logs to CSV file...")
    # Iterate over the rows and format binary fields
    for row in rows:
        formatted_row = [format_binary_data(value) for value in row]  # Format each field
        csv_writer.writerow(formatted_row)

# Close the connection
cur.close()
conn.close()

if __name__ == "__main__":
    pass
