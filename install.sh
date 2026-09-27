#!/bin/sh
# Install or upgrade `ta` from this repository's GitHub Releases, using your
# own `gh` login (the releases are not public).
#
#   gh api repos/calebl/text-adventure-terminal/contents/install.sh \
#     -H 'Accept: application/vnd.github.raw' | sh
#
# Environment:
#   TA_VERSION      a release tag such as v0.1.0 (default: the latest release)
#   TA_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
#   TA_RELEASE_REPO owner/name to install from (default: calebl/text-adventure-terminal)
set -eu

repo="${TA_RELEASE_REPO:-calebl/text-adventure-terminal}"
install_dir="${TA_INSTALL_DIR:-$HOME/.local/bin}"
version="${TA_VERSION:-}"

say() { printf '%s\n' "$*"; }
fail() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

command -v gh >/dev/null 2>&1 || fail "the GitHub CLI (gh) is required: https://cli.github.com"
gh auth status >/dev/null 2>&1 || fail "gh is not signed in; run: gh auth login"
command -v tar >/dev/null 2>&1 || fail "tar is required"

case "$(uname -s)" in
  Linux) os=unknown-linux-gnu ;;
  Darwin) os=apple-darwin ;;
  *) fail "unsupported system $(uname -s); on Windows use install.ps1" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) fail "unsupported machine $(uname -m)" ;;
esac
target="$arch-$os"
asset="ta-$target.tar.gz"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

if [ -z "$version" ]; then
  version="$(gh release view --repo "$repo" --json tagName --jq .tagName)" ||
    fail "could not find a release in $repo (does your gh login have access?)"
fi
say "Downloading ta $version for $target"
gh release download "$version" --repo "$repo" --dir "$tmp" \
  --pattern "$asset" --pattern SHA256SUMS ||
  fail "could not download $asset from $repo $version"

expected="$(awk -v a="$asset" '{ n=$2; sub(/^\*/, "", n) } n == a { print $1 }' "$tmp/SHA256SUMS")"
[ -n "$expected" ] || fail "SHA256SUMS does not list $asset"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$asset" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$tmp/$asset" | awk '{ print $1 }')"
else
  fail "need sha256sum or shasum to verify the download"
fi
[ "$actual" = "$expected" ] || fail "checksum mismatch for $asset; nothing was installed"

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$install_dir"
# Replace by rename so a running `ta` keeps its old file.
cp "$tmp/ta" "$install_dir/.ta.new"
chmod 755 "$install_dir/.ta.new"
mv -f "$install_dir/.ta.new" "$install_dir/ta"
say "Installed ta $version to $install_dir/ta"

case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) say "Add $install_dir to your PATH, for example in ~/.profile:"
     say "  export PATH=\"$install_dir:\$PATH\"" ;;
esac
