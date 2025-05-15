if [ -d "./zksync-working" ] && [ -n "$(ls -A ./zksync-working)" ]; then
    echo "zksync-working found and is not empty. Cleaning containers and restarting services..."
    cd zksync-working
    zkstack dev clean containers && zkstack up -o false
    cd ..
fi


if [ -z "$(ls -A ./zksync-old)" ]; then
    echo "Moving zksync-working to zksync-old"
    mv ./zksync-working ./zksync-old
else
    if [ -z "$(ls -A ./zksync-new)" ]; then
        echo "Moving zksync-working to zksync-new"
        mv ./zksync-working ./zksync-new
    else
        echo "Both zksync-old and zksync-new contain files. No reset action taken."
    fi
fi