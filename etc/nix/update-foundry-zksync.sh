#!/usr/bin/env bash

# Set the version
VERSION="v0.0.15"

# Function to update SHA in the nix file
update_sha() {
    local platform="$1"
    local new_sha="$2"
    local file="./foundry-zksync.nix"

    case "$platform" in
        "linux_amd64")
            if [[ "$OSTYPE" == "darwin"* ]]; then
                sed -i "" "s/linux_amd_bin_sha = \".*\"/linux_amd_bin_sha = \"$new_sha\"/" "$file"
            else
                sed -i "s/linux_amd_bin_sha = \".*\"/linux_amd_bin_sha = \"$new_sha\"/" "$file"
            fi
            ;;
        "linux_arm64")
            if [[ "$OSTYPE" == "darwin"* ]]; then
                sed -i "" "s/linux_arm_bin_sha = \".*\"/linux_arm_bin_sha = \"$new_sha\"/" "$file"
            else
                sed -i "s/linux_arm_bin_sha = \".*\"/linux_arm_bin_sha = \"$new_sha\"/" "$file"
            fi
            ;;
        "darwin_amd64")
            if [[ "$OSTYPE" == "darwin"* ]]; then
                sed -i "" "s/darwin_amd_bin_sha = \".*\"/darwin_amd_bin_sha = \"$new_sha\"/" "$file"
            else
                sed -i "s/darwin_amd_bin_sha = \".*\"/darwin_amd_bin_sha = \"$new_sha\"/" "$file"
            fi
            ;;
        "darwin_arm64")
            if [[ "$OSTYPE" == "darwin"* ]]; then
                sed -i "" "s/darwin_arm_bin_sha = \".*\"/darwin_arm_bin_sha = \"$new_sha\"/" "$file"
            else
                sed -i "s/darwin_arm_bin_sha = \".*\"/darwin_arm_bin_sha = \"$new_sha\"/" "$file"
            fi
            ;;
    esac
}

if ! [[ -f ./foundry-zksync.nix ]]; then
    echo "Error: ./foundry-zksync.nix not found"
    exit 1
fi

# Update version in the file if provided as argument
if [ "$1" ]; then
    VERSION="$1"
    if [[ "$OSTYPE" == "darwin"* ]]; then
        sed -i "" "s/version *= *\".*\"/version = \"$VERSION\"/" ./foundry-zksync.nix
    else
        sed -i "s/version *= *\".*\"/version = \"$VERSION\"/" ./foundry-zksync.nix
    fi
fi

echo "Updating foundry-zksync to version $VERSION..."

# Fetch and update each platform
echo "Fetching Linux AMD64..."
SHA=$(nix-prefetch-url "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${VERSION}/foundry_zksync_${VERSION}_linux_amd64.tar.gz" 2>/dev/null)
update_sha "linux_amd64" "$SHA"

echo "Fetching Linux ARM64..."
SHA=$(nix-prefetch-url "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${VERSION}/foundry_zksync_${VERSION}_linux_arm64.tar.gz" 2>/dev/null)
update_sha "linux_arm64" "$SHA"

echo "Fetching Darwin AMD64..."
SHA=$(nix-prefetch-url "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${VERSION}/foundry_zksync_${VERSION}_darwin_amd64.tar.gz" 2>/dev/null)
update_sha "darwin_amd64" "$SHA"

echo "Fetching Darwin ARM64..."
SHA=$(nix-prefetch-url "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${VERSION}/foundry_zksync_${VERSION}_darwin_arm64.tar.gz" 2>/dev/null)
update_sha "darwin_arm64" "$SHA"

echo "Update complete!"
