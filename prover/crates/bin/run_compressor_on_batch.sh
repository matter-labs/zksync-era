#!/bin/bash
# Script to run proof compressor on files

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to display errors
error() {
    echo -e "${RED}ERROR: $1${NC}" >&2
    exit 1
}

# Function to display info messages
info() {
    echo -e "${GREEN}INFO: $1${NC}"
}

# Function to display warnings
warn() {
    echo -e "${YELLOW}WARN: $1${NC}"
}

# Check arguments
if [ $# -lt 2 ]; then
    echo "Usage: $0 <scheduler_proof_file> <aux_witness_file> [options]"
    echo ""
    echo "Required:"
    echo "  scheduler_proof_file      Path to scheduler proof file"
    echo "  aux_witness_file          Path to auxiliary witness file"
    echo ""
    echo "Optional:"
    echo "  --output <path>           Output file path (default: ./compressed_proof.bin)"
    echo "  --setup-data <path>       Setup data directory (required for compression)"
    echo "  --universal-setup <path>  Universal setup file (required for compression)"
    echo "  --fflonk                  Use FFLONK wrapper (default: Plonk)"
    echo ""
    echo "Example:"
    echo "  $0 scheduler_proof.bin aux_witness.bin \\"
    echo "    --setup-data /mnt/prover-setup-data \\"
    echo "    --universal-setup /mnt/prover-setup-data/setup_2^26.key"
    exit 1
fi

SCHEDULER_PROOF_FILE="$1"
AUX_WITNESS_FILE="$2"
shift 2

# Default values
OUTPUT_FILE="./compressed_proof.bin"
SETUP_DATA_PATH=""
UNIVERSAL_SETUP_PATH=""
FFLONK_FLAG=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --output)
            OUTPUT_FILE="$2"
            shift 2
            ;;
        --setup-data)
            SETUP_DATA_PATH="$2"
            shift 2
            ;;
        --universal-setup)
            UNIVERSAL_SETUP_PATH="$2"
            shift 2
            ;;
        --fflonk)
            FFLONK_FLAG="--fflonk"
            shift
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# Validate input files exist
if [ ! -f "$SCHEDULER_PROOF_FILE" ]; then
    error "Scheduler proof file not found: $SCHEDULER_PROOF_FILE"
fi

if [ ! -f "$AUX_WITNESS_FILE" ]; then
    error "Auxiliary witness file not found: $AUX_WITNESS_FILE"
fi

info "Input files validated:"
info "  - Scheduler proof: $SCHEDULER_PROOF_FILE ($(du -h "$SCHEDULER_PROOF_FILE" | cut -f1))"
info "  - Aux witness: $AUX_WITNESS_FILE ($(du -h "$AUX_WITNESS_FILE" | cut -f1))"

# Validate required parameters for compression
if [ -z "$SETUP_DATA_PATH" ]; then
    error "Setup data path is required (use --setup-data)"
fi

if [ -z "$UNIVERSAL_SETUP_PATH" ]; then
    error "Universal setup path is required (use --universal-setup)"
fi

# Validate setup data exists
if [ ! -d "$SETUP_DATA_PATH" ]; then
    error "Setup data directory not found: $SETUP_DATA_PATH"
fi

if [ ! -f "$UNIVERSAL_SETUP_PATH" ]; then
    error "Universal setup file not found: $UNIVERSAL_SETUP_PATH"
fi

# Check GPU availability
if ! command -v nvidia-smi &> /dev/null; then
    warn "nvidia-smi not found. GPU might not be available."
else
    info "GPU check:"
    nvidia-smi --query-gpu=name,memory.total --format=csv,noheader | head -1
fi

# Run compressor
info "Starting proof compression..."

COMPRESS_CMD="cargo run --release --bin zksync_proof_compressor_file_runner -- \
    --scheduler-proof $SCHEDULER_PROOF_FILE \
    --aux-witness $AUX_WITNESS_FILE \
    --output $OUTPUT_FILE \
    --setup-data-path $SETUP_DATA_PATH \
    --universal-setup-path $UNIVERSAL_SETUP_PATH \
    $FFLONK_FLAG"

info "Running: $COMPRESS_CMD"
eval $COMPRESS_CMD || error "Failed to run compressor"

info "Compression completed successfully!"
info "  - Output file: $OUTPUT_FILE ($(du -h "$OUTPUT_FILE" | cut -f1))"

info "Done! All operations completed successfully."
