#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)"
cargo clippy --all-targets --all-features --tests --all \
    -- -D warnings
