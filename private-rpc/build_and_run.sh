docker build -t private-rpc .
docker run --rm \
  --env-file example.env \
  --publish 4041:4041 \
  --add-host=host.docker.internal:host-gateway \
  private-rpc

