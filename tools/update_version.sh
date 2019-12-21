#!/usr/bin/env bash

ROOT="$(realpath "$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )/..")"

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 version" >&2
  exit 1
fi

version=$1

IFS=$'\n' find "${ROOT}" -name Cargo.toml | while read -r f; do
  echo "Update $f"
  sed -i "s/^version =.*/version = \"${version}\"/" "$f"
done