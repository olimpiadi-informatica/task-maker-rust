#!/usr/bin/env bash

server_args=${SERVER_ARGS:-}
worker_args=${WORKER_ARGS:-}
server_addr=${SERVER_ADDR:-127.0.0.1:27183}
spawn_server=${SPAWN_SERVER:-true}
spawn_worker=${SPAWN_WORKER:-true}

export RUST_LOG=info
export RUST_BACKTRACE=1

server_store=$(mktemp -d tmserver.XXXXXXX -p /tmp)
worker_store=$(mktemp -d tmworker.XXXXXXX -p /tmp)

function spawn_server() {
  task-maker-rust --store-dir "$server_store" $server_args --server
}
function spawn_worker() {
  task-maker-rust --store-dir "$worker_store" $worker_args --worker $server_addr
}

# worker only
if [[ $spawn_server != true && $spawn_worker == true ]]; then
  spawn_worker
# server only
elif [[ $spawn_server == true && $spawn_worker != true ]]; then
  spawn_server
# server+worker
elif [[ $spawn_server == true && $spawn_worker == true ]]; then
  # run the workers in background, but wait for the server
  ( sleep 2s && spawn_worker ) &
  spawn_server
# nothing to spawn
else
  bash
fi