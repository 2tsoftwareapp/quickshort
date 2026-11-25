#!/bin/sh
# Fail when the retired GitHub owner appears in active product surfaces.
# Historical migration notes live under specs/ and are intentionally not scanned.
set -eu

matches=$(git grep -n 'truongnat' -- \
  crates relay packaging .github docs README.md LAUNCH.md demo appshot-video \
  2>/dev/null || true)

if [ -n "$matches" ]; then
  echo "legacy GitHub identity 'truongnat' found in active surfaces:" >&2
  printf '%s\n' "$matches" >&2
  echo "migrate active links/metadata to '2tsoftwareapp' or move historical notes under specs/" >&2
  exit 1
fi
