#!/bin/bash

set -o errexit
set -o pipefail


api_endpoint="https://api.github.com/users/zksync-era-bot"
wait_time=60
max_retries=60
retry_count=0

while [[ $retry_count -lt $max_retries ]]; do
  response=$(run_retried curl -s -w "%{http_code}" -o temp.json "$api_endpoint")
  http_code=$(echo "$response" | tail -n1)

  if [[ "$http_code" == "200" ]]; then
    echo "Request successful. Not rate-limited."
    cat temp.json
    rm temp.json
    exit 0
  elif [[ "$http_code" == "403" ]]; then
    rate_limit_exceeded=$(jq -r '.message' temp.json | grep -i "API rate limit exceeded")
    if [[ -n "$rate_limit_exceeded" ]]; then
      retry_count=$((retry_count+1))
      echo "API rate limit exceeded. Retry $retry_count of $max_retries. Retrying in $wait_time seconds..."
      sleep $wait_time
    else
      echo "Request failed with HTTP status $http_code."
      cat temp.json
      rm temp.json
      exit 1
    fi
  else
    echo "Request failed with HTTP status $http_code."
    cat temp.json
    rm temp.json
    exit 1
  fi
done

echo "Reached the maximum number of retries ($max_retries). Exiting."
rm temp.json
exit 1
