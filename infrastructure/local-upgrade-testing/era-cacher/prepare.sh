mkdir upgrade-testing
cd upgrade-testing

git clone https://github.com/matter-labs/zksync-era.git zksync-old
git clone https://github.com/matter-labs/zksync-era.git zksync-new

cd zksync-old
git checkout main
git submodule update --init --recursive
cd ..

cd zksync-new
git checkout vg/local-upgrade-testing
git submodule update --init --recursive
cd ..


cp -r zksync-new/infrastructure/local-upgrade-testing/era-cacher .