import csv
import sys

# ANSI color codes
COLOR_RED = '\033[91m'
COLOR_GREEN = '\033[92m'
COLOR_YELLOW = '\033[93m'
COLOR_BLUE = '\033[94m'
COLOR_MAGENTA = '\033[95m'
COLOR_CYAN = '\033[96m'
COLOR_RESET = '\033[0m'  # Reset to default color

def print_colored(text, color):
    # Print text in the specified color
    print(color + text + COLOR_RESET)

def compare_benchmarks(filename1,filename2):
    file1_data = get_file_data(filename1)
    file2_data = get_file_data(filename2)

    for name in file1_data:
        if name in file2_data:
            print_colored(f'{name}', COLOR_CYAN)
            print(f'Used zkevm ergs: {file1_data[name][0]}, {file2_data[name][0]}, ',end='') 
            print_colored(f'{(file1_data[name][0] - file2_data[name][0]) / file1_data[name][0] * 100} %', COLOR_RED if file1_data[name][0] < file2_data[name][0] else COLOR_GREEN)
            print(f'Used evm gas: {file1_data[name][1]}, {file2_data[name][1]}, ',end='') 
            print_colored(f'{(file1_data[name][1] - file2_data[name][1]) / file1_data[name][1] * 100} %', COLOR_RED if file1_data[name][1] < file2_data[name][1] else COLOR_GREEN)
            print(f'Used circuits: {file1_data[name][2]}, {file2_data[name][2]}, ',end='') 
            print_colored(f'{(file1_data[name][2] - file2_data[name][2]) / file1_data[name][2] * 100} %', COLOR_RED if file1_data[name][2] < file2_data[name][2] else COLOR_GREEN)
            print()
        else:
            print_colored(f'{name} is not in {filename2}', COLOR_RED)
            print()

    for name in file2_data:
        if name not in file1_data:
            print_colored(f'{name} is not in {filename1}', COLOR_RED)
            print()

def get_file_data(filename):
    with open(filename, newline='') as file:
        file_data = {}
        reader = csv.DictReader(file)
        for row in reader:
            # Access the values of each column using the column names
            name = row['name']
            used_zkevm_ergs = int(row['used_zkevm_ergs'])
            used_evm_gas = int(row['used_evm_gas'])
            used_circuits = float(row['used_circuits'])
            file_data[name] = (used_zkevm_ergs, used_evm_gas, used_circuits)
    return file_data

def main():
    if len(sys.argv) < 3:
        print("Usage: python compare_benchmarks.py <file1.csv> <file2.csv>")
        return
    compare_benchmarks(sys.argv[1],sys.argv[2])

main()
    


