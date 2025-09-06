#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'


server_args=${SERVER_ARGS:-}
worker_args=${WORKER_ARGS:-}
server_addr=${SERVER_ADDR:-127.0.0.1:27183}
spawn_server=${SPAWN_SERVER:-true}
spawn_worker=${SPAWN_WORKER:-true}
tm_loglevel=${TM_LOGLEVEL:-info}

# default: nproc - 1
default_nworkers=$(( $(nproc) - 1 ))
nworkers=$default_nworkers
help_flag=false

#################### helpers
function check_posint() {
  local re='^[0-9]+$'
  local mynum="$1"
  local option="$2"

  if ! [[ "$mynum" =~ $re ]] ; then
     (echo -n "Error in option '$option': " >&2)
     (echo "must be a positive integer, got $mynum." >&2)
     exit 1
  fi

  if ! [ "$mynum" -gt 0 ] ; then
     (echo "Error in option '$option': must be positive, got $mynum." >&2)
     exit 1
  fi
}
#################### end: helpers

#################### help
function short_usage() {
  (>&2 echo \
"Usage:
  $(basename "$0") [options]")
}

function usage() {
  (>&2 short_usage )
  (>&2 echo \
"
Entrypoint for task-maker-rust Docker image.

Options:
  -j, --jobs NWORKERS    Number of workers to launch [default: <nproc>-1].
  -h, --help             Show this help and exits.
")
}

# the leading ":" lets us handle errors ourselves
while getopts ":hj:-:" OPT; do
  if [ "$OPT" = "-" ]; then
    # Long option handling
    longopt="${OPTARG%%=*}"
    if [ "$longopt" != "$OPTARG" ]; then
      # do not use
      (echo "Error. Invalid option: --$OPTARG. " \
            "Use --opt OPT instead of --opt=OPT." >&2)
      exit 1
    else
      # form: --long VALUE  (value may be in next argv)
      OPTARG=
    fi

    case "$longopt" in
      help)
        OPT="h"
        ;;

      jobs)
        OPT="j"
        # If no "=VALUE", try to take the next argv as the argument
        next="${!OPTIND}"
        if [ -n "$next" ] && [[ "$next" != -* ]]; then
          OPTARG="$next"
          OPTIND=$((OPTIND + 1))
        fi
        ;;

      *)
        echo "Error. Bad long option --$longopt" >&2
        exit 1
        ;;
    esac
  fi

  case "$OPT" in
    h)
      help_flag=true
      ;;

    j)
      check_posint "$OPTARG" '-j/--jobs'
      nworkers="$OPTARG"
      ;;

    \?)
      echo "Error. Invalid option: -$OPTARG" >&2
      exit 1
      ;;

    :)
      echo "Error. Option $OPTARG requires an argument." >&2
      exit 1
      ;;
  esac
done

if $help_flag; then
  usage
  exit 0
fi
#################### end: help

server_store=$(mktemp -d tmserver.XXXXXXX -p /tmp)

# create nworkers worker stores directories
worker_base=$(mktemp -u tmworker.XXXXXXX -p /tmp)
declare -a worker_stores=()
for ((i=1; i<=nworkers; i++)); do
  idx=$(printf "%02d" "$i")
  dir="${worker_base}-${idx}"
  mkdir -p "$dir"
  worker_stores+=("$dir")
done

# cleanup exit trap
function cleanup {
  # remove server and worker stores
  rm -rf /tmp/tmserver.*
  for wstore in $(find /tmp -type d -name 'tmworker.*'); do
    rm -rf $wstore
  done
}
trap cleanup EXIT

function loglevel_verbosity_flag() {
  case "$1" in
    error)
      echo ""
      ;;
    warn)
      echo "-v"
      ;;
    info)
      echo "-vv"
      ;;
    debug)
      echo "-vvv"
      ;;
    *)
      (echo "Unknown level: $1. Default to 'error'." >&2);
      return 1
      ;;
  esac
}

function spawn_tmserver() {
  local v="$1"

  if [[ "${tm_loglevel}" == 'debug' ]]; then
    set -x
  fi

  task-maker-tools ${v:+$v} \
    server \
      --store-dir "$server_store" \
        $server_args
}

function spawn_tmworker() {
  local v="$1"
  local store_dir="$2"

  if [[ "${tm_loglevel}" == 'debug' ]]; then
    set -x
  fi

  task-maker-tools ${v:+$v} \
    worker \
      --store-dir "$store_dir" \
        $worker_args \
        "$server_addr"
}

verbosity_flag=$(loglevel_verbosity_flag "${tm_loglevel}")

# worker only
if [[ $spawn_server == false && $spawn_worker == true ]]; then
  for ((i=0; i<${#worker_stores[@]}; i++)); do
    spawn_tmworker "$verbosity_flag" "${worker_stores[$i]}" &
  done
  wait

# server only
elif [[ $spawn_server == true && $spawn_worker == false ]]; then
  spawn_tmserver "$verbosity_flag"

# server+worker
elif [[ $spawn_server == true && $spawn_worker == true ]]; then
  # run the workers in background, but wait for the server
  (
    sleep 2s
    for ((i=0; i<${#worker_stores[@]}; i++)); do
      spawn_tmworker "$verbosity_flag" "${worker_stores[$i]}" &
    done
    wait
  ) &
  spawn_tmserver "$verbosity_flag"

# nothing to spawn
else
  bash
fi

exit 0
