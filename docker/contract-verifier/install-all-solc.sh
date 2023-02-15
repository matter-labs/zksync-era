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
