#!/usr/bin/env bash
# Write THIRD-PARTY-LICENSES for one release target: the licence texts of
# every crate built into `ta` for that target.
#
#   .github/licenses/third-party.sh <target> <output file>
#
# cargo-about does the work. It drops a crate that one dependency declares
# twice under different platforms (keyring's macOS and iOS
# security-framework), so the crate list is checked against `cargo tree` and
# any crate it missed gets its own licence files appended from its source.
set -euo pipefail

target="$1"
out="$2"
here="$(dirname "$0")"

cargo about generate --locked --target "$target" \
  -c "$here/about.toml" -o "$out" "$here/about.hbs"

metadata="$(cargo metadata --locked --format-version 1)"

missing=0
while read -r name version; do
  grep -q "^  $name $version\( \|$\)" "$out" && continue
  manifest="$(jq -r --arg n "$name" --arg v "$version" \
    '.packages[] | select(.name == $n and .version == $v) | .manifest_path' <<<"$metadata")"
  command -v cygpath >/dev/null && manifest="$(cygpath -u "$manifest")"
  dir="$(dirname "$manifest")"
  files="$(find "$dir" -maxdepth 1 -type f \( -iname 'LICEN[CS]E*' -o -iname 'COPYING*' \) | sort)"
  if [ -z "$files" ]; then
    echo "third-party.sh: no licence file for $name $version in $dir" >&2
    missing=1
    continue
  fi
  echo "third-party.sh: cargo-about missed $name $version; adding its licence files" >&2
  {
    printf '%s\n' '================================================================================'
    printf '%s %s\n\n' "$name" "$version"
    while read -r f; do
      cat "$f"
      printf '\n'
    done <<<"$files"
  } >>"$out"
done < <(cargo tree --locked -e normal --target "$target" --prefix none -f '{p}' |
  awk '$1 != "text-adventure-terminal" { sub(/^v/, "", $2); print $1, $2 }' | sort -u)

exit "$missing"
