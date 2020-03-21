#!/usr/bin/env bash

processes=$(ps aux)

# check server
echo $processes | grep task-maker-rust | grep -- --server 2>/dev/null >/dev/null
server_ok=$?

# check worker
echo $processes | grep task-maker-rust | grep -- --worker 2>/dev/null >/dev/null
worker_ok=$?

if [[ $server_ok == 0 && $worker_ok == 0 ]]; then
  echo "server & worker ok"
  exit 0
fi
if [[ $server_ok == 0 ]]; then
  echo "worker down"
  exit 1
fi
if [[ $worker_ok == 0 ]]; then
  echo "server down"
  exit 1
fi
echo "server & worker down"
exit 1
