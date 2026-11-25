#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

spawn_server=${SPAWN_SERVER:-true}
spawn_worker=${SPAWN_WORKER:-true}

task_maker_processes="$(pgrep -f 'task-maker-tools' \
                       | xargs --no-run-if-empty ps -o 'command' \
                       | tail -n+2)"

# check server
echo "$task_maker_processes" | grep server 2>&1 >/dev/null
server_ok=$?

# check worker
# - read nworkers file or die
nworkers=$(cat 'nworkers' || exit 1)
# - if nworkers is not set ot null, default to 0
nworkers="${nworkers:-0}"
# - get number of workers running
nworkers_running=$(echo "$task_maker_processes" | grep -c worker)

if [[ $spawn_server && $spawn_worker ]] && [[ $server_ok == 0 ]] \
    && [[ $nworkers_running == $nworkers ]]; then
  (echo "server and workers ok (${nworkers_running} workers running)" >&2)
  exit 0
fi
if $spawn_server && [[ $server_ok != 0 ]]; then
  (echo "server down" >&2)
  exit 1
fi
if $spawn_worker && [[ $nworkers_running != $nworkers ]]; then
  (echo "some workers down, expected ${nworkers}, ${nworkers_running} running" >&2)
  exit 1
fi
(echo "server and workers down" >&2)
exit 1
