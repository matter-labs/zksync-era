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