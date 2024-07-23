if [ ! -s $1 ]; then
  /usr/bin/entrypoint.sh generate-secrets > $1
fi
