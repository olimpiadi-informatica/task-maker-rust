#!/usr/bin/env bash

version="$(grep '^version' ../../../Cargo.toml | cut -d'"' -f 2)"
hash=$(curl -L "https://github.com/olimpiadi-informatica/task-maker-rust/archive/v${version}.tar.gz" | sha256sum | cut -d' ' -f 1)
sed "s/@@VERSION@@/$version/g" < PKGBUILD | sed "s/@@SHA256@@/$hash/"
