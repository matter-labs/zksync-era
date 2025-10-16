#!/bin/bash
# Script to run proof compressor on files
#
# Prerequisites (install manually):
# - yarn: npm install -g yarn
# - forge (foundry-zksync): curl -L https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/install-foundry-zksync | bash
#
# What this script does:
# - Installs zkstack CLI (if needed)
# - Downloads prover keys via zkstack
# - Downloads compressor keys via zkstack
# - Initializes bellman-cuda
# - Runs the compressor

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

# Default values
SCHEDULER_PROOF_FILE="./proofs_fri_proof_21680346_123.bin"
AUX_WITNESS_FILE="./scheduler_witness_jobs_fri_aux_output_witness_22951_123.bin"
OUTPUT_FILE="./compressed_proof.bin"
SETUP_DATA_PATH=""
UNIVERSAL_SETUP_PATH=""
FFLONK_FLAG=""
SKIP_SETUP=false

# Show usage if --help is passed
if [[ "$1" == "--help" ]] || [[ "$1" == "-h" ]]; then
    echo "Usage: $0 [options]"
    echo ""
    echo "Prerequisites (install manually first):"
    echo "  yarn:  npm install -g yarn"
    echo "  forge: curl -L https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/install-foundry-zksync | bash"
    echo ""
    echo "Options:"
    echo "  --scheduler-proof <path>  Path to scheduler proof file (default: ./proofs_fri_proof_21680346_123.bin)"
    echo "  --aux-witness <path>      Path to auxiliary witness file (default: ./scheduler_witness_jobs_fri_aux_output_witness_22951_123.bin)"
    echo "  --output <path>           Output file path (default: ./compressed_proof.bin)"
    echo "  --setup-data <path>       Setup data directory (required)"
    echo "  --universal-setup <path>  Universal setup file (required)"
    echo "  --fflonk                  Use FFLONK wrapper (default: Plonk)"
    echo "  --skip-setup              Skip zkstack setup (keys download and bellman-cuda init)"
    echo ""
    echo "Example:"
    echo "  $0 --setup-data /mnt/prover-setup-data \\"
    echo "    --universal-setup /mnt/prover-setup-data/setup_2^26.key"
    exit 0
fi

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --scheduler-proof)
            SCHEDULER_PROOF_FILE="$2"
            shift 2
            ;;
        --aux-witness)
            AUX_WITNESS_FILE="$2"
            shift 2
            ;;
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
        --skip-setup)
            SKIP_SETUP=true
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

# Note: Setup data directory will be created by zkstack during setup
# Universal setup path will be validated before compression

# Check GPU availability
if ! command -v nvidia-smi &> /dev/null; then
    warn "nvidia-smi not found. GPU might not be available."
else
    info "GPU check:"
    nvidia-smi --query-gpu=name,memory.total --format=csv,noheader | head -1
fi

# Setup zkstack and keys if not skipped
if [ "$SKIP_SETUP" = false ]; then
    info "Setting up zkstack and keys..."
    
    # Try to load zkstack from common locations
    if [ -f "$HOME/.zshenv" ]; then
        source "$HOME/.zshenv" 2>/dev/null || true
    fi
    if [ -f "$HOME/.bashrc" ]; then
        source "$HOME/.bashrc" 2>/dev/null || true
    fi
    
    # Add common zkstack paths to PATH
    export PATH="$HOME/.local/bin:$PATH"
    
    # Check if zkstack is installed
    if command -v zkstack &> /dev/null; then
        ZKSTACK_VERSION=$(zkstack --version 2>/dev/null || echo "unknown")
        ZKSTACK_PATH=$(which zkstack)
        info "✅ zkstack is already installed: $ZKSTACK_VERSION"
        info "   Location: $ZKSTACK_PATH"
    else
        info "zkstack not found, installing..."
        info "Installing zkstackup..."
        curl -L https://raw.githubusercontent.com/matter-labs/zksync-era/main/zkstack_cli/zkstackup/install | bash
        
        # Add zkstackup to PATH for current session
        export PATH="$HOME/.local/bin:$PATH"
        
        # Verify zkstackup installation
        if ! command -v zkstackup &> /dev/null; then
            error "Failed to install zkstackup. Try manual installation."
        fi
        
        info "Installing zkstack..."
        zkstackup
        
        # Add zkstack to PATH for current session
        export PATH="$HOME/.local/bin:$PATH"
        
        # Verify zkstack installation
        if ! command -v zkstack &> /dev/null; then
            error "Failed to install zkstack. PATH may need to be updated. Try running: source ~/.zshenv && zkstackup"
        fi
        
        # Display zkstack version
        ZKSTACK_VERSION=$(zkstack --version 2>/dev/null || echo "unknown")
        info "✅ zkstack installed successfully: $ZKSTACK_VERSION"
    fi
    
    # Check prerequisites are installed
    info "Checking prerequisites..."
    
    # Add common paths to PATH
    export PATH="$HOME/.foundry/bin:$HOME/.yarn/bin:$HOME/.cargo/bin:/usr/local/bin:$PATH"
    
    # Check yarn
    if ! command -v yarn &> /dev/null; then
        error "yarn is not installed! Please install yarn first.
        See README.md or https://yarnpkg.com/getting-started/install"
    fi
    YARN_VER=$(yarn --version 2>/dev/null || echo "error")
    if [ "$YARN_VER" = "error" ]; then
        error "yarn found but doesn't work properly: $(which yarn)
        Please reinstall yarn correctly."
    fi
    info "✅ yarn: $YARN_VER ($(which yarn))"
    
    # Check forge
    if ! command -v forge &> /dev/null; then
        error "forge is not installed! Please install foundry-zksync first.
        Run: curl -L https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/install-foundry-zksync | bash"
    fi
    
    FORGE_PATH=$(which forge)
    info "✅ forge found: $(forge --version | head -1)"
    info "   Location: $FORGE_PATH"
    
    # Ensure all prerequisites are in PATH before calling zkstack
    info "Verifying prerequisites are in PATH..."
    info "forge: $(which forge 2>/dev/null || echo 'not found')"
    info "yarn: $(which yarn 2>/dev/null || echo 'not found')"
    
    # Verify forge is foundry-zksync by checking 'forge build --help'
    if command -v forge &> /dev/null; then
        info "Verifying forge is foundry-zksync..."
        FORGE_HELP=$(forge build --help 2>&1 || true)
        if echo "$FORGE_HELP" | grep -q "ZKSync configuration"; then
            info "✅ forge is foundry-zksync (zkstack-compatible)"
        else
            error "❌ forge is NOT foundry-zksync! zkstack requires foundry-zksync, not standard foundry.
    Please install: curl -L https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/install-foundry-zksync | bash"
        fi
    else
        error "forge not found in PATH!"
    fi
    
    # Try to create symlinks in /usr/local/bin if prerequisites are not there
    if command -v forge &> /dev/null; then
        FORGE_SRC=$(which forge)
        if [ ! -f "/usr/local/bin/forge" ] || [ ! -L "/usr/local/bin/forge" ]; then
            info "Creating symlink for forge in /usr/local/bin..."
            if sudo ln -sf "$FORGE_SRC" /usr/local/bin/forge 2>/dev/null; then
                info "✅ Symlink created: /usr/local/bin/forge -> $FORGE_SRC"
            else
                warn "⚠️  Cannot create symlink (no sudo access)"
            fi
        else
            info "Symlink already exists: /usr/local/bin/forge"
        fi
        
        # Verify symlink works with zkstack's check
        if [ -f "/usr/local/bin/forge" ] || [ -L "/usr/local/bin/forge" ]; then
            if /usr/local/bin/forge --version &>/dev/null; then
                # Also verify it's foundry-zksync
                SYMLINK_FORGE_HELP=$(/usr/local/bin/forge build --help 2>&1 || true)
                if echo "$SYMLINK_FORGE_HELP" | grep -q "ZKSync configuration"; then
                    info "✅ /usr/local/bin/forge is working and is foundry-zksync"
                else
                    warn "⚠️  /usr/local/bin/forge works but is NOT foundry-zksync!"
                fi
            else
                warn "⚠️  /usr/local/bin/forge exists but doesn't work"
            fi
        fi
    fi
    
    if command -v yarn &> /dev/null; then
        YARN_SRC=$(which yarn)
        
        # Check if yarn actually works before creating symlink
        if yarn --version &>/dev/null; then
            if [ ! -f "/usr/local/bin/yarn" ] && [ ! -L "/usr/local/bin/yarn" ]; then
                info "Creating symlink for yarn in /usr/local/bin..."
                if sudo ln -sf "$YARN_SRC" /usr/local/bin/yarn 2>/dev/null; then
                    info "✅ Symlink created: /usr/local/bin/yarn -> $YARN_SRC"
                else
                    warn "⚠️  Cannot create symlink (no sudo access)"
                fi
            else
                info "Symlink already exists: /usr/local/bin/yarn"
            fi
            
            # Verify symlink works
            if [ -f "/usr/local/bin/yarn" ] || [ -L "/usr/local/bin/yarn" ]; then
                if /usr/local/bin/yarn --version &>/dev/null; then
                    info "✅ /usr/local/bin/yarn is working"
                else
                    warn "⚠️  /usr/local/bin/yarn exists but doesn't work"
                    # Remove broken symlink
                    sudo rm -f /usr/local/bin/yarn 2>/dev/null || true
                fi
            fi
        else
            warn "⚠️  yarn found at $YARN_SRC but doesn't work, skipping symlink creation"
        fi
    fi
    
    # Final diagnostic before calling zkstack
    info "═══════════════════════════════════════════════════════"
    info "Final environment check before calling zkstack:"
    info "  which forge: $(which forge 2>/dev/null || echo 'NOT FOUND')"
    info "  which yarn:  $(which yarn 2>/dev/null || echo 'NOT FOUND')"
    info "  /usr/local/bin/forge exists: $([ -e /usr/local/bin/forge ] && echo 'YES' || echo 'NO')"
    info "  /usr/local/bin/yarn exists:  $([ -e /usr/local/bin/yarn ] && echo 'YES' || echo 'NO')"
    
    # Test zkstack's check manually
    info "Testing zkstack's forge check manually..."
    if which forge &>/dev/null; then
        FORGE_BUILD_HELP=$(forge build --help 2>&1 || echo "ERROR")
        if echo "$FORGE_BUILD_HELP" | grep -q "ZKSync configuration"; then
            info "  ✅ 'forge build --help' contains 'ZKSync configuration'"
        else
            warn "  ❌ 'forge build --help' does NOT contain 'ZKSync configuration'"
            warn "     This is exactly what zkstack checks! Forge is not foundry-zksync."
        fi
    else
        warn "  ❌ 'which forge' failed - forge not in PATH"
    fi
    info "═══════════════════════════════════════════════════════"
    
    # Setup prover keys
    info "Setting up prover keys..."
    zkstack prover setup-keys || warn "Failed to setup prover keys (continuing anyway)"
    
    # Setup compressor keys
    info "Setting up compressor keys..."
    zkstack prover compressor-keys || warn "Failed to setup compressor keys (continuing anyway)"
    
    # Initialize bellman-cuda
    info "Initializing bellman-cuda..."
    zkstack prover init-bellman-cuda || warn "Failed to initialize bellman-cuda (continuing anyway)"
    
    info "Setup completed"
else
    info "Skipping zkstack setup (--skip-setup flag provided)"
fi

# Validate setup data exists before running compressor
if [ ! -d "$SETUP_DATA_PATH" ]; then
    error "Setup data directory not found: $SETUP_DATA_PATH. Keys may not have been downloaded correctly."
fi

if [ ! -f "$UNIVERSAL_SETUP_PATH" ]; then
    error "Universal setup file not found: $UNIVERSAL_SETUP_PATH"
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
