#!/usr/bin/env bash
# Prints the CHANGELOG.md notes for version $1 (without the heading). Fails when there are none.
set -euo pipefail
version="$1"
notes=$(awk -v v="$version" 'index($0, "## [" v "]") == 1 { found = 1; next } found && /^## \[/ { exit } found { print }' CHANGELOG.md | sed '/./,$!d')
if [ -z "$notes" ]; then echo "::error::CHANGELOG.md has no notes under ## [$version]" >&2; exit 1; fi
printf '%s\n' "$notes"
