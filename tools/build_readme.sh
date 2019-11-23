#!/usr/bin/env bash

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"
cargo readme > README.md
