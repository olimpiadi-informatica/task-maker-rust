#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

task_maker_processes=$(pgrep -f 'task-maker-tools' \
                       | xargs --no-run-if-empty ps -o 'command' \
                       | tail -n+2)

# check server
echo $task_maker_processes | grep server 2>&1 >/dev/null
server_ok=$?

# check worker
echo $task_maker_processes | grep server 2>&1 >/dev/null
worker_ok=$?

if [[ $server_ok == 0 && $worker_ok == 0 ]]; then
  echo "server and worker ok"
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
echo "server and worker down"
exit 1
