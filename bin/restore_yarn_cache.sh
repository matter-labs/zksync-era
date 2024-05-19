#!/usr/bin/env bash

for node_dir in . etc/system-contracts etc/contracts-test-data contracts;
do
  mkdir -p $node_dir
  cp -r ../node_modules_backup/$node_dir $node_dir/node_modules
done
