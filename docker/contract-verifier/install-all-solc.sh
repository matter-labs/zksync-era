# Installs all solc versions from the github repo
wget -O list.txt https://github.com/ethereum/solc-bin/raw/gh-pages/linux-amd64/list.txt

# Iterate versions
for LN in $(cat list.txt)
do
    # Download
    wget https://github.com/ethereum/solc-bin/raw/gh-pages/linux-amd64/$LN

    # Get short version name
    temp="${LN#"solc-linux-amd64-v"}"
    version="${temp%\+*}"

    # Move and rename
    mkdir -p etc/solc-bin/$version/
    mv $LN etc/solc-bin/$version/solc
    chmod +x etc/solc-bin/$version/solc

    ls etc/solc-bin/
done

# Download zkVM solc
for version in $(curl -s https://api.github.com/repos/matter-labs/era-solidity/releases?per_page=200 | jq -r '.[].tag_name')
do
    mkdir -p etc/solc-bin/zkVM-$version/
    wget https://github.com/matter-labs/era-solidity/releases/download/$version/solc-linux-amd64-$version -O etc/solc-bin/zkVM-$version/solc
    chmod +x etc/solc-bin/zkVM-$version/solc
done
