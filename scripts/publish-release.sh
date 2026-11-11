#!/usr/bin/env bash
# Creates the draft GitHub release for $TAG from the build artifacts in bundles/ and writes the
# updater manifest latest.json. Needs GH_TOKEN, TAG and GITHUB_REPOSITORY; run from the repo root.
# Uses only jq and gh, which the GitHub runners have.
set -euo pipefail

v="${TAG#v}"
mkdir -p release-assets
find bundles -type f -exec mv -t release-assets {} +

# The updater expects the name tauri-action used for the macOS bundle.
mv release-assets/Aloud.app.tar.gz "release-assets/Aloud_${v}_aarch64.app.tar.gz"
mv release-assets/Aloud.app.tar.gz.sig "release-assets/Aloud_${v}_aarch64.app.tar.gz.sig"

notes=$(bash scripts/release-notes.sh "$v")

base="https://github.com/$GITHUB_REPOSITORY/releases/download/$TAG"
entry() {
  jq -n --arg url "$base/$1" --rawfile sig "release-assets/$1.sig" '{url: $url, signature: ($sig | rtrimstr("\n"))}'
}
linux=$(entry "Aloud_${v}_amd64.AppImage")
nsis=$(entry "Aloud_${v}_x64-setup.exe")
msi=$(entry "Aloud_${v}_x64_en-US.msi")
mac=$(entry "Aloud_${v}_aarch64.app.tar.gz")
jq -n --arg v "$v" --arg notes "$notes" --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson linux "$linux" --argjson nsis "$nsis" --argjson msi "$msi" --argjson mac "$mac" \
  '{version: $v, notes: $notes, pub_date: $date, platforms: {
      "linux-x86_64": $linux, "linux-x86_64-appimage": $linux,
      "windows-x86_64": $nsis, "windows-x86_64-nsis": $nsis, "windows-x86_64-msi": $msi,
      "darwin-aarch64": $mac, "darwin-aarch64-app": $mac}}' > release-assets/latest.json

# A re-run replaces the draft of an unpublished release; a published one is never touched.
if is_draft=$(gh release view "$TAG" -R "$GITHUB_REPOSITORY" --json isDraft --jq .isDraft 2>/dev/null); then
  if [ "$is_draft" = true ]; then
    gh release delete "$TAG" --yes -R "$GITHUB_REPOSITORY"
  else
    echo "::error::Release $TAG is already published" >&2
    exit 1
  fi
fi

prerelease=()
[[ "$TAG" == *-* ]] && prerelease=(--prerelease)
gh release create "$TAG" release-assets/* --draft --verify-tag --title "Aloud $TAG" --notes "$notes" \
  "${prerelease[@]}" -R "$GITHUB_REPOSITORY"
