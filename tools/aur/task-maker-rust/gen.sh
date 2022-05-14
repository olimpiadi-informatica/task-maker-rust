#!/usr/bin/env bash

version="$(grep '^version' ../../../Cargo.toml | cut -d'"' -f 2)"
sed "s/@@VERSION@@/$version/g" < PKGBUILD