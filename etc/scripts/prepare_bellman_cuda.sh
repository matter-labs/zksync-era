echo "preparing bellman cuda directory"
gh release -R github.com/matter-labs/bellman-cuda download "$1"
gh release -R github.com/matter-labs/bellman-cuda download "$1" -A tar.gz
mkdir -p bellman-cuda
tar xvf bellman-cuda.tar.gz -C ./bellman-cuda
tar xvf bellman-cuda-"$1".tar.gz
mv bellman-cuda-"$1"/* ./bellman-cuda/
