#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)"

cargo clippy --all-targets --all-features --tests --all -- \
    -D warnings \
    `# https://rust-lang.github.io/rust-clippy/master/#string_slice` \
    -D clippy::string_slice
