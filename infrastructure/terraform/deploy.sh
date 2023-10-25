#!/bin/bash

SSH_KEY=$1

# Check if SSH key parameter is provided
if [[ -z "$SSH_KEY" ]]; then
    echo "Please provide an SSH key."
    exit 1
fi

INSTANCE_TYPE:=c6a.xlarge
S3_BUCKET:=terraform-zksync
S3_DIR:=zksync_operator
S3_REGION:=us-west-2

terraform -chdir=terraform/example_sequencer_nodes/ init  \
							 -var ssh_key_name=$(SSH_KEY_NAME) \
							 -var instance_type=$(INSTANCE_TYPE) \
							 -var cluster_name=$(CLUSTER_NAME) \
							 -backend-config="bucket=$(S3_BUCKET)" \
							 -backend-config="key=$(S3_DIR)/terraform.tfstate" \
							 -backend-config="region=$(S3_REGION)"
@sleep 30
