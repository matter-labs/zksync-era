#!/usr/bin/env bash
set -e

rm -rf db
zk db reset
zk server --genesis
