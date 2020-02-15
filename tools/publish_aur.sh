#!/usr/bin/env bash

set -e

AUR_REPO="ssh://aur@aur.archlinux.org/task-maker-rust.git"

version=$1
if [ -z "$version" ]; then
  echo "Usage: $0 version" >&2
  exit 1
fi
if [[ "$version" == v* ]]; then
  echo "Version should not start with 'v'" >&2
  exit 1
fi

WORKDIR=$(mktemp -d)
function cleanup() {
    rm -rf "${WORKDIR}"
}
trap cleanup EXIT

cd "$WORKDIR"

git clone "${AUR_REPO}" .
sed -i "s/pkgver=.*/pkgver=$version/" PKGBUILD
sed -i "s/pkgrel=.*/pkgrel=1/" PKGBUILD
updpkgsums
makepkg --printsrcinfo > .SRCINFO
git diff
read -p "Continue (y/n)?" -r choice
case "$choice" in
  y|Y ) ;;
  * ) echo "Aborting"; exit 3;;
esac

git commit -am "Version $version"
git push origin master
