if [ -z "$(ls -A ./zksync-old)" ]; then
    mv ./zksync-working ./zksync-old
else
    if [ -z "$(ls -A ./zksync-new)" ]; then
        mv ./zksync-working ./zksync-new
    else
        echo "Both zksync-old and zksync-new contain files. No reset action taken."
    fi
fi