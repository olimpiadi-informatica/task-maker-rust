#!/usr/bin/env bash

if [[ ! -f package.json ]]; then
  echo "You should run this script from bindings/ts/"
  exit 1
fi

echo "Installing node dependencies"
npm install

echo "Generating typescript definitions from the rust code"
cargo run --bin task-maker-tools typescriptify | npx prettier --parser typescript > src/task_maker.d.ts

echo "Generating the JSON schemas"
npx ts-node tools/gen-schema.ts