#!/bin/bash

version=$1
old_version=`sed -n 's/^version = "\(.\+\)"/\1/p' cpp/Cargo.toml`

echo "updating $old_version to $version"

for toml in */Cargo.toml; do
  cp $toml $toml.bk
  cat $toml.bk \
    | sed -e 's/\(\(^\|cpp_.\+\)version = \"=\?\)'$old_version'/\1'$version'/g' \
    > $toml
  rm $toml.bk
done

