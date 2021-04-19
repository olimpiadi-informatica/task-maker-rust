#!/usr/bin/env bash

set -ex

# where the data files will be put
export TM_DATA_DIR=/usr/share/task-maker-rust

# rustup
source $HOME/.cargo/env

# move to the directory where the source code will be present
cd /source
if [ ! -f "Cargo.toml" ]; then
  echo "Mount the repo inside /source!" >&2
  exit 1
fi

# build the binary
cargo build --bin task-maker --release

# build the autocompletion files
cargo run --release --bin task-maker-gen-autocompletion

# prepare the .deb file
cargo deb --no-build