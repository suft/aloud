#!/usr/bin/env bash
# Maintainer script: mirrors the ONNX Runtime builds that ort-sys downloads from cdn.pyke.io to a
# GitHub release of this repo, so builds don't depend on that CDN. Run it again whenever the
# `ort` crate is bumped to a version that pins a different ONNX Runtime build, then update the
# table in scripts/fetch-onnxruntime.ts with the `name size sha256` lines it prints.
#
# Needs: bash, curl, sha256sum, xz, GNU tar, gzip and an authenticated `gh`. Usage:
#   scripts/mirror-onnxruntime.sh
#
# The pyke archives are raw LZMA2 streams (dictionary 1<<26) holding a tar. If `xz --format=raw`
# can't decode one, decode it with a throwaway Rust program outside the repo instead
# (lzma-rust2 =0.15.8, `Lzma2Reader::new(file, 1 << 26, None)` piped into `tar -x`, as ort-sys does
# in build/download/extract.rs) and keep the rest of the script as is.
set -euo pipefail

VERSION=1.31.0
REPO=suft/aloud
TAG="onnxruntime-$VERSION"
BASE="https://cdn.pyke.io/0/pyke:ort-rs/ms@$VERSION"

# target | pyke file | pyke sha256 | mirror asset
ROWS=(
  "x86_64-unknown-linux-gnu|x86_64-unknown-linux-gnu.tar.lzma2|02aac0d3b4927026292088986d7a377c657c88761b1c068be217a67b35447678|onnxruntime-$VERSION-x86_64-unknown-linux-gnu.tar.gz"
  "x86_64-pc-windows-msvc|x86_64-pc-windows-msvc+directml.tar.lzma2|f99aaef23f12986abb05924c944d8433521263ad756b462df762a5a982c98405|onnxruntime-$VERSION-x86_64-pc-windows-msvc-directml.tar.gz"
  "aarch64-apple-darwin|aarch64-apple-darwin+coreml.tar.lzma2|4033efca0e6c1606bd2ec3870d4cb19d0dba0ad0160dbfb616e66cae969ed68c|onnxruntime-$VERSION-aarch64-apple-darwin-coreml.tar.gz"
)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
out="$work/out"
mkdir -p "$out"

assets=()
notes="ONNX Runtime $VERSION builds mirrored from cdn.pyke.io, where ort-sys downloads them (builds by pyke.io; ONNX Runtime is MIT-licensed). Each asset is the pyke archive repacked as .tar.gz with its contents at the archive root; see scripts/fetch-onnxruntime.ts."$'\n'

for row in "${ROWS[@]}"; do
  IFS='|' read -r target file pyke_sha asset <<<"$row"
  echo "== $target"
  curl -fL --retry 3 -o "$work/$file" "$BASE/$file"
  echo "$pyke_sha  $work/$file" | sha256sum --check --status || {
    echo "$file: checksum mismatch" >&2
    exit 1
  }

  dir="$work/x-$target"
  mkdir -p "$dir"
  xz --decompress --format=raw --lzma2=dict=64MiB --stdout <"$work/$file" | tar -x -C "$dir"

  tar --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 -cf - -C "$dir" . |
    gzip -n -9 >"$out/$asset"

  echo "$asset $(stat -c %s "$out/$asset") $(sha256sum "$out/$asset" | cut -d' ' -f1)"
  assets+=("$out/$asset")
  notes+=$'\n'"- $asset: $BASE/$file (sha256 $pyke_sha)"
done

gh release create "$TAG" -R "$REPO" --prerelease --latest=false \
  --title "ONNX Runtime $VERSION (build mirror)" --notes "$notes" "${assets[@]}"
