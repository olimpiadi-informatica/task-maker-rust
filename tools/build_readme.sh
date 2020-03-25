#!/usr/bin/env bash

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"
cargo readme -i src/main.rs > README.md
